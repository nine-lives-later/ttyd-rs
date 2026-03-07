const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const esbuild = require('esbuild');

console.log('Building frontend...');

// Make sure dist directory exists
const distDir = path.join(__dirname, 'dist');
if (!fs.existsSync(distDir)) {
    fs.mkdirSync(distDir);
}

console.log('Building CSS with Tailwind...');
execSync('npx @tailwindcss/cli -i ./src/style.css -o ./dist/style.css --minify', { stdio: 'inherit' });
const tailwindCss = fs.readFileSync(path.join(distDir, 'style.css'), 'utf-8');

console.log('Bundling JS with esbuild...');
fs.writeFileSync(path.join(__dirname, 'empty.js'), 'module.exports = {};');

esbuild.buildSync({
    entryPoints: ['./src/index.js'],
    bundle: true,
    minify: true,
    outfile: './dist/bundle.js',
    alias: {
        'child_process': path.join(__dirname, 'empty.js'),
        'util': require.resolve('util/')
    }
});
const appJs = fs.readFileSync(path.join(distDir, 'bundle.js'), 'utf-8');

console.log('Reading XTerm CSS...');
const xtermCss = fs.readFileSync(path.join(__dirname, 'node_modules', '@xterm', 'xterm', 'css', 'xterm.css'), 'utf-8');

console.log('Injecting into template...');
const template = fs.readFileSync(path.join(__dirname, 'src', 'template.html'), 'utf-8');
const outputHtml = template
    .split('{{XTERM_CSS}}').join(xtermCss)
    .split('{{TAILWIND_CSS}}').join(tailwindCss)
    .split('{{APP_JS}}').join(appJs);

const outputPath = path.join(__dirname, '..', 'src', 'assets', 'index.html');
fs.writeFileSync(outputPath, outputHtml);

console.log(`Successfully built and wrote to ${outputPath}`);
