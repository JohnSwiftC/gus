# GuS - A Windows RAT Built in Rust

The following capabilities will be coming soon ~

- File upload (can do this already with PowerShell!)

A much better version of my first RAT/Reverse Shell GuShell. Built with Rust, proper networking (works with netcat, no bulky listener), persistence, light-weight and easier to build and configure than ever. Stay connected, stay hidden.

Small bonus, AV is horrible at detecting Rust binaries.

# Features

- Drop into PowerShell, CMD, or any other shell application.
- Keylog to a log file or a Discord Webhook
- Maintain persistence disguised as a Windows process in the registry and AppData.
- Appears to not function upon first run.
- Always seeks a connection while the machine is on.

# Usage

Clone repo, look at main.rs and set your configuration settings, run `cargo build --release`, deploy.

Set up your netcat listener and type the `help` command for options on your current build.

Current Commands

`help` > Show help menu

`shell` > Drop into a shell

`exit` > When in a shell, exit the shell process

`keylog` > Start a keylogger process **(WARNING)**: Do NOT run more than once per session, it WILL mess things up.

# Warning

Like anything I build, this is for education purposes and fun! I am not responsible for the misuse of this tool.