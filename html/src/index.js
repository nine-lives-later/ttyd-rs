import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { ClipboardAddon } from '@xterm/addon-clipboard';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { ImageAddon } from '@xterm/addon-image';
import { SearchAddon } from '@xterm/addon-search';
import { SerializeAddon } from '@xterm/addon-serialize';
import { setupProtocols } from './trzsz';

const term = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    macOptionIsMeta: true,
    scrollback: 10000,
    fontFamily: '"JetBrains Mono", Consolas, "Courier New", monospace',
});

const fitAddon = new FitAddon();
term.loadAddon(fitAddon);

const webLinksAddon = new WebLinksAddon();
term.loadAddon(webLinksAddon);

const clipboardAddon = new ClipboardAddon();
term.loadAddon(clipboardAddon);

const unicode11Addon = new Unicode11Addon();
term.loadAddon(unicode11Addon);
term.unicode.activeVersion = '11';

const imageAddon = new ImageAddon();
term.loadAddon(imageAddon);

const searchAddon = new SearchAddon();
term.loadAddon(searchAddon);

const serializeAddon = new SerializeAddon();
term.loadAddon(serializeAddon);

const container = document.getElementById('terminal-container');

let socket;
let protocolHandler = null;

function connectTerminal(authToken) {
    document.fonts.ready.then(() => {
        term.open(container);
        fitAddon.fit();
        term.focus();
        applyTheme();

        try {
            const webglAddon = new WebglAddon();
            webglAddon.onContextLoss(() => {
                webglAddon.dispose();
            });
            term.loadAddon(webglAddon);
            console.log('[ttyd] WebGL renderer enabled');
        } catch (e) {
            console.warn('[ttyd] WebGL addon failed to load, falling back to DOM', e);
        }

        // Now that term is open and has an element, we can setup sockets and protocols
        const protocol = (window.location.protocol === 'https:') ? 'wss://' : 'ws://';
        const port = window.location.port ? `:${window.location.port}` : '';
        const path = window.location.pathname.replace(/\/$/, '');
        const wsUrl = `${protocol}${window.location.hostname}${port}${path}/ws`;

        socket = new WebSocket(wsUrl, ['tty']);
        socket.binaryType = 'arraybuffer';

        socket.onopen = () => {
            protocolHandler = setupProtocols(term, socket);

            if (authToken) {
                socket.send(JSON.stringify({ authToken: authToken }));
            }

            // Send init message
            const initMsg = JSON.stringify({
                columns: term.cols,
                rows: term.rows,
            });
            socket.send(initMsg);
        };

        socket.onmessage = (event) => {
            if (event.data instanceof ArrayBuffer) {
                const data = new Uint8Array(event.data);
                if (data.length > 0) {
                    if (data[0] === 48) { // '0'
                        if (protocolHandler) {
                            protocolHandler.processServerOutput(event.data.slice(1));
                        } else {
                            term.write(data.slice(1));
                        }
                    } else if (data[0] === 49) { // '1'
                        const title = new TextDecoder().decode(data.slice(1));
                        document.title = title;
                    } else if (data[0] === 50) { // '2'
                        // preferences, ignore for now
                    }
                }
            } else if (typeof event.data === 'string') {
                const type = event.data.charCodeAt(0);
                if (type === 48) { // '0'
                    const encoder = new TextEncoder();
                    const slice = encoder.encode(event.data.substring(1));
                    if (protocolHandler) {
                        protocolHandler.processServerOutput(slice.buffer);
                    } else {
                        term.write(event.data.substring(1));
                    }
                } else if (type === 49) { // '1'
                    document.title = event.data.substring(1);
                } else if (type === 50) { // '2'
                    // preferences
                }
            }
        };

        socket.onclose = () => {
            term.write('\r\n\x1b[31mConnection Closed\x1b[0m\r\n');
        };

        socket.onerror = (err) => {
            term.write('\r\n\x1b[31mWebSocket Error\x1b[0m\r\n');
        };
    });
}

let copyTimeout = null;
function showCopyNotification() {
    const notif = document.getElementById('copy-notification');
    if (!notif) return;
    
    notif.style.display = 'flex';
    // Small delay to allow display to apply before fading in
    requestAnimationFrame(() => {
        notif.style.opacity = '1';
    });
    
    if (copyTimeout) clearTimeout(copyTimeout);
    copyTimeout = setTimeout(() => {
        notif.style.opacity = '0';
        setTimeout(() => {
            if (notif.style.opacity === '0') {
                notif.style.display = 'none';
            }
        }, 300); // Wait for transition to finish
    }, 3000);
}

term.onSelectionChange(() => {
    const text = term.getSelection();
    if (text && text.trim().length > 0) {
        navigator.clipboard.writeText(text).then(() => {
            showCopyNotification();
        }).catch(err => {
            console.error('[ttyd] Failed to copy to clipboard automatically', err);
        });
    }
});

term.onData(data => {
    if (socket && socket.readyState === WebSocket.OPEN) {
        const encoder = new TextEncoder();
        const msgBytes = encoder.encode(data);
        const payload = new Uint8Array(msgBytes.length + 1);
        payload[0] = 48; // '0'
        payload.set(msgBytes, 1);
        socket.send(payload);
    }
});

term.onResize(size => {
    if (socket && socket.readyState === WebSocket.OPEN) {
        const resizeMsg = JSON.stringify({
            columns: size.cols,
            rows: size.rows,
        });
        const encoder = new TextEncoder();
        const msgBytes = encoder.encode(resizeMsg);
        const payload = new Uint8Array(msgBytes.length + 1);
        payload[0] = 49; // '1'
        payload.set(msgBytes, 1);
        socket.send(payload);
    }
});

term.attachCustomKeyEventHandler(event => {
    if (event.type !== 'keydown') {
        return true;
    }

    // Ctrl+Shift+C to Copy
    if (event.ctrlKey && event.shiftKey && (event.key === 'C' || event.key === 'c' || event.code === 'KeyC')) {
        event.preventDefault();
        const text = term.getSelection();
        if (text) {
            navigator.clipboard.writeText(text).then(() => {
                showCopyNotification();
            }).catch(err => {
                console.error('[ttyd] Failed to copy to clipboard', err);
            });
        }
        return false;
    }

    // Ctrl+Shift+V to Paste
    if (event.ctrlKey && event.shiftKey && (event.key === 'V' || event.key === 'v' || event.code === 'KeyV')) {
        event.preventDefault();
        navigator.clipboard.readText().then(text => {
            if (socket && socket.readyState === WebSocket.OPEN) {
                const encoder = new TextEncoder();
                const msgBytes = encoder.encode(text);
                const payload = new Uint8Array(msgBytes.length + 1);
                payload[0] = 48; // '0'
                payload.set(msgBytes, 1);
                socket.send(payload);
            }
        }).catch(err => {
            console.error('[ttyd] Failed to read from clipboard', err);
        });
        return false;
    }
    
    // Ctrl+Shift+F to Search
    if (event.ctrlKey && event.shiftKey && (event.key === 'F' || event.key === 'f' || event.code === 'KeyF')) {
        event.preventDefault();
        const query = prompt('Search terminal buffer:');
        if (query) {
            searchAddon.findNext(query);
        }
        return false;
    }
    
    // Ctrl+Shift+S to Save (Serialize)
    if (event.ctrlKey && event.shiftKey && (event.key === 'S' || event.key === 's' || event.code === 'KeyS')) {
        event.preventDefault();
        const text = serializeAddon.serialize();
        const blob = new Blob([text], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'terminal-export.txt';
        a.click();
        URL.revokeObjectURL(url);
        return false;
    }

    return true;
});

// Help Modal logic
const helpBtn = document.getElementById('help-btn');
const helpModal = document.getElementById('help-modal');
const closeHelpBtn = document.getElementById('close-help-btn');

function showHelp() {
    helpModal.style.display = 'flex';
}

function hideHelp() {
    helpModal.style.display = 'none';
}

if (helpBtn && helpModal && closeHelpBtn) {
    helpBtn.addEventListener('click', showHelp);
    closeHelpBtn.addEventListener('click', hideHelp);
    
    // Close modal when clicking outside of the modal content
    helpModal.addEventListener('click', (e) => {
        if (e.target === helpModal) {
            hideHelp();
        }
    });

    // Close on escape key
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && helpModal.style.display === 'flex') {
            hideHelp();
        }
    });
}

// Theme logic
const themeBtn = document.getElementById('theme-btn');
const themeIconAuto = document.getElementById('theme-icon-auto');
const themeIconDark = document.getElementById('theme-icon-dark');
const themeIconLight = document.getElementById('theme-icon-light');

const themes = ['auto', 'dark', 'light'];
let currentTheme = localStorage.getItem('ttyd-theme') || 'auto';

const xtermDarkTheme = {
    background: '#000000',
    foreground: '#d1d5db',
    cursor: '#d1d5db',
    selectionBackground: 'rgba(255, 255, 255, 0.3)',
};

const xtermLightTheme = {
    background: '#ffffff',
    foreground: '#1f2937',
    cursor: '#1f2937',
    selectionBackground: 'rgba(0, 0, 0, 0.3)',
};

function applyTheme() {
    let isDark = false;
    if (currentTheme === 'auto') {
        isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    } else {
        isDark = currentTheme === 'dark';
    }

    if (isDark) {
        document.documentElement.classList.add('dark');
        term.options.theme = xtermDarkTheme;
    } else {
        document.documentElement.classList.remove('dark');
        term.options.theme = xtermLightTheme;
    }

    if (themeIconAuto && themeIconDark && themeIconLight) {
        themeIconAuto.classList.add('hidden');
        themeIconDark.classList.add('hidden');
        themeIconLight.classList.add('hidden');

        if (currentTheme === 'auto') {
            themeIconAuto.classList.remove('hidden');
            themeBtn.title = 'Theme: Auto';
        } else if (currentTheme === 'dark') {
            themeIconDark.classList.remove('hidden');
            themeBtn.title = 'Theme: Dark';
        } else {
            themeIconLight.classList.remove('hidden');
            themeBtn.title = 'Theme: Light';
        }
    }
}

if (themeBtn) {
    themeBtn.addEventListener('click', () => {
        const currentIndex = themes.indexOf(currentTheme);
        currentTheme = themes[(currentIndex + 1) % themes.length];
        localStorage.setItem('ttyd-theme', currentTheme);
        applyTheme();
    });
}

window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (currentTheme === 'auto') {
        applyTheme();
    }
});

fetch('/config')
    .then(r => r.json())
    .then(config => {
        const versionEl = document.getElementById('ttyd-version');
        if (versionEl) versionEl.textContent = config.version;

        if (config.auth_mode === 'token') {
            const urlParams = new URLSearchParams(window.location.search);
            const queryToken = urlParams.get('token');

            if (queryToken) {
                // Clear the token from the URL bar for security, keeping other params
                urlParams.delete('token');
                const newUrl = window.location.pathname + (urlParams.toString() ? '?' + urlParams.toString() : '');
                window.history.replaceState({}, document.title, newUrl);
                
                connectTerminal(queryToken);
            } else {
                const loginModal = document.getElementById('login-modal');
                const loginBtn = document.getElementById('login-submit-btn');
                const loginInput = document.getElementById('login-token-input');

                loginModal.style.display = 'flex';
                
                const submitLogin = () => {
                    const token = loginInput.value.trim();
                    if (!token) return;
                    loginModal.style.display = 'none';
                    connectTerminal(token);
                };

                loginBtn.addEventListener('click', submitLogin);
                loginInput.addEventListener('keydown', (e) => {
                    if (e.key === 'Enter') submitLogin();
                });
            }
        } else {
            connectTerminal(null);
        }
    })
    .catch(err => {
        console.error("Failed to fetch config", err);
        connectTerminal(null);
    });
