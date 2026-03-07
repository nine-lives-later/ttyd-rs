import { TrzszFilter } from 'trzsz';
import * as Zmodem from 'zmodem.js/src/zmodem_browser';

function bytesHuman(bytes, precision) {
    if (isNaN(bytes)) return '-';
    if (bytes === 0) return '0';
    precision = precision || 1;
    const units = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB'];
    const num = Math.floor(Math.log(bytes) / Math.log(1024));
    const value = (bytes / Math.pow(1024, Math.floor(num))).toFixed(precision);
    return `${value} ${units[num]}`;
}

export function setupProtocols(term, socket) {
    const sender = (data) => {
        if (socket.readyState === WebSocket.OPEN) {
            // we must prepend '0' for normal data as per our protocol
            if (typeof data === 'string') {
                const encoder = new TextEncoder();
                const msgBytes = encoder.encode(data);
                const payload = new Uint8Array(msgBytes.length + 1);
                payload[0] = 48; // '0'
                payload.set(msgBytes, 1);
                socket.send(payload);
            } else {
                const arr = new Uint8Array(data);
                const payload = new Uint8Array(arr.length + 1);
                payload[0] = 48; // '0'
                payload.set(arr, 1);
                socket.send(payload);
            }
        }
    };

    const writer = (data) => {
        if (typeof data === 'string') {
            term.write(data);
        } else {
            term.write(new Uint8Array(data));
        }
    };

    let session = null;
    let sentry = null;
    let denier = null;

    const reset = () => {
        session = null;
        denier = null;
        term.options.disableStdin = false;
        term.focus();
    };

    const writeProgress = (offer) => {
        const file = offer.get_details();
        const name = file.name;
        const size = file.size;
        const offset = offer.get_offset();
        const percent = ((100 * offset) / size).toFixed(2);
        writer(`${name} ${percent}% ${bytesHuman(offset, 2)}/${bytesHuman(size, 2)}\r`);
    };

    const receiveFile = () => {
        session.on('offer', offer => {
            offer.on('input', () => writeProgress(offer));
            offer.accept().then(payloads => {
                Zmodem.Browser.save_to_disk(payloads, offer.get_details().name);
            }).catch(() => reset());
        });
        session.start();
    };

    const onSend = () => {
        const fileInput = document.createElement('input');
        fileInput.type = 'file';
        fileInput.multiple = true;
        fileInput.style.display = 'none';
        document.body.appendChild(fileInput);
        
        fileInput.addEventListener('change', (e) => {
            const files = e.target.files;
            if (files && files.length > 0) {
                Zmodem.Browser.send_files(session, files, {
                    on_progress: (_, offer) => writeProgress(offer),
                }).then(() => session.close())
                  .catch(() => reset());
            } else {
                reset();
            }
            document.body.removeChild(fileInput);
        });
        
        fileInput.addEventListener('cancel', () => {
            reset();
            document.body.removeChild(fileInput);
        });
        
        fileInput.click();
    };

    const zmodemDetect = (detection) => {
        term.options.disableStdin = true;
        denier = () => detection.deny();
        session = detection.confirm();
        session.on('session_end', () => { denier = null; reset(); });

        if (session.type === 'send') {
            onSend();
        } else {
            receiveFile();
        }
    };

    sentry = new Zmodem.Sentry({
        to_terminal: octets => writer(new Uint8Array(octets)),
        sender: octets => sender(new Uint8Array(octets)),
        on_retract: () => reset(),
        on_detect: detection => zmodemDetect(detection),
    });

    const trzszFilter = new TrzszFilter({
        writeToTerminal: data => {
            if (!trzszFilter.isTransferringFiles()) {
                let octets;
                if (typeof data === 'string') {
                    octets = new TextEncoder().encode(data);
                } else {
                    octets = new Uint8Array(data);
                }
                try {
                    sentry.consume(octets);
                } catch (e) {
                    console.error('[ttyd] zmodem consume: ', e);
                    reset();
                }
            } else {
                writer(typeof data === 'string' ? data : new Uint8Array(data));
            }
        },
        sendToServer: data => sender(data),
        terminalColumns: term.cols,
        isWindowsShell: false,
        dragInitTimeout: 3000,
    });

    term.onResize(size => {
        trzszFilter.setTerminalColumns(size.cols);
    });

    const element = term.element;
    element.addEventListener('dragover', event => event.preventDefault());
    element.addEventListener('drop', event => {
        event.preventDefault();
        trzszFilter.uploadFiles(event.dataTransfer?.items)
            .then(() => console.log('[ttyd] trzsz upload success'))
            .catch(err => console.log('[ttyd] trzsz upload failed: ' + err));
    });

    term.onKey(e => {
        const event = e.domEvent;
        if (event.ctrlKey && event.key === 'c') {
            if (denier) { try { denier(); } catch(err) {} denier = null; }
            if (session) { try { session.abort(); } catch(err) {} }
        }
    });

    return {
        processServerOutput: (data) => trzszFilter.processServerOutput(data)
    };
}