This project is a web console (tty) project written in Rust using xterm js (https://github.com/xtermjs/xterm.js) and Tailwind CSS.

General rules:
- always commit your changes to Git.
- if the Git repository is unclean, simply commit all changes.
- never create new branches! 
- never push your changes!

# Rust Backend

use "cargo build" to fix any errors.

The Rust backend build requires the ./src/assets/index.html build artifact from the HTML frontend. 
Make sure to build the HTML frontend first before building the Rust backend.

never use dyn, instead add a generic type parameter where needed.

after commit run "cargo fix --all-targets" and "cargo fmt" and commit again, if sth was changed.

when updating crates, install the cargo-edit tool (if missing) and run the "cargo upgrade --incompatible" command and fix any issues.

# HTML Frontend

The frontend part is inside the ./html folder.
use "npm run build" to fix any errors.

when updating nodejs packages, do so one by one and test if the updated version is working by running "npm run build". 
do not fix any errors, instead try a previous version. if that does not work, skip updating that particular package.

never add global modules. 

never use the "npm install" flags "--force" or "--legacy-peer-deps". if there are package errors, then resolve them by updating the packages.
