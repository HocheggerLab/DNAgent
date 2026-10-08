# Installing DNAgent

Current builds are **macOS on Apple Silicon** (M1 and later) and **Windows x64**. Intel
Macs and Linux are not built yet; on those, [build from source](#build-from-source).

DNAgent comes in two parts, and they are useful separately:

- the **desktop app**, for looking at and editing constructs;
- the **`dnagent` command-line tool**, which does the biology and is what an AI agent
  drives.

Install both if you want the agent workflows; the app alone is enough to open plasmids.

## Install on macOS

Paste these into Terminal. Use the commands rather than downloading in a browser —
the reason is [below](#why-the-curl-command).

```bash
# Desktop app
curl -fsSL https://github.com/HocheggerLab/DNAgent/releases/latest/download/DNAgent-macos-arm64.tar.gz | tar xz -C /Applications

# Command-line tool
curl -fsSL https://github.com/HocheggerLab/DNAgent/releases/latest/download/dnagent-cli-macos-arm64.tar.gz | tar xz -C /usr/local/bin
```

Check it worked:

```bash
open -a DNAgent
dnagent --version
```

If `dnagent: command not found`, `/usr/local/bin` is not on your `PATH`. Add it:

```bash
echo 'export PATH="/usr/local/bin:$PATH"' >> ~/.zshrc && exec zsh
```

### Why the curl command

DNAgent is not signed with an Apple Developer certificate, because that costs £79 a year
and this is free research software. macOS treats unsigned apps harshly: since macOS 15 it
refuses to open one downloaded from the web, calling it **"damaged"**, and the old
Control-click→Open workaround no longer works.

The quarantine flag that triggers this is set by the *downloading application* — Safari,
Chrome, Mail. `curl` does not set it. So an app fetched with the command above is not
quarantined and simply opens.

"Unsigned" means Apple has not checked who published it. It does not mean the app is
modified or unsafe; the build is produced by [a public GitHub Actions
workflow](../.github/workflows/release.yml) from the commit its tag names, and each
release lists SHA-256 checksums.

If you did download the `.dmg` in a browser, clear the flag once:

```bash
xattr -dr com.apple.quarantine /Applications/DNAgent.app
```

## Install on Windows

Download from the [latest release](https://github.com/HocheggerLab/DNAgent/releases/latest):

- **`DNAgent-windows-x64-setup.exe`** — the app. Run it; it installs to your user account,
  no administrator rights needed.
- **`dnagent-cli-windows-x64.zip`** — the command-line tool. Unzip `dnagent.exe` into a
  folder on your `PATH`, or make one:

  ```powershell
  mkdir "$env:LOCALAPPDATA\Programs\DNAgent"
  # move dnagent.exe there, then add it to PATH for future terminals:
  [Environment]::SetEnvironmentVariable(
      "Path", "$env:Path;$env:LOCALAPPDATA\Programs\DNAgent", "User")
  ```

  Open a new terminal and check with `dnagent --version`.

### The SmartScreen warning

DNAgent is not code-signed — a Windows certificate is a yearly cost this project does not
carry. The first time you run the installer, SmartScreen says the publisher is unknown:
click **More info**, then **Run anyway**. You should only do that because you can see
where the file came from: each release is built by [a public
workflow](../.github/workflows/release.yml) from the commit its tag names, and lists
SHA-256 checksums. Verify one if you like:

```powershell
Get-FileHash .\DNAgent-windows-x64-setup.exe -Algorithm SHA256
```

Unlike macOS, the `curl` trick does not help here: Windows marks a file by the zone it
came from, not by what downloaded it.

### What is missing on Windows

The **live agent connection is macOS and Linux only**. It runs over a unix socket, which
Windows does not have, and the portable alternatives are network ports that any
process on the machine could reach — that needs an access token before it is safe to
serve your constructs over one.

Everything else is the same: the app, every CLI command, reading and writing SnapGene
`.dna` and GenBank. For agent work on Windows, use the **file handoff** — the agent reads
and writes constructs in your workspace folder, and the app picks them up. See [agent
handoff](agent-handoff.md).

## Connect an agent (macOS and Linux)

This is what makes DNAgent different from a viewer: an agent in your terminal can see what
you have open and put results on your screen. With [Claude
Code](https://claude.com/claude-code) installed:

```bash
claude mcp add dnagent -- dnagent mcp
```

That registers `dnagent mcp`, a small relay between the agent and the running app. Start
DNAgent, open a construct, then ask the agent something like *"what am I looking at?"* — it
should answer with the construct name and your selection.

The agent sees only DNAgent: the constructs you have open, your selection, and the files it
writes into your workspace folder (`~/DNAgent` by default). The connection is a user-only
socket on your own machine; nothing is sent anywhere.

`dnagent mcp --help` explains the relay and its socket. What the agent can do through it,
and the file-based alternative when no app is running, are in [agent
handoff](agent-handoff.md).

For Claude Desktop (Cowork), other MCP clients, and why ChatGPT cannot connect, see
[Connect an agent](../README.md#connect-an-agent) in the README.

## Updating

Re-run the install command — `tar` overwrites in place. Quit DNAgent first if it is
running.

## Uninstalling

```bash
# macOS
rm -rf /Applications/DNAgent.app /usr/local/bin/dnagent
rm -rf ~/Library/Application\ Support/DNAgent   # feature library and enzyme catalogue
```

On Windows, uninstall the app from **Settings → Apps**, delete `dnagent.exe`, and remove
`%APPDATA%\DNAgent` for the feature library and enzyme catalogue.

Your constructs are your own files and are never stored inside the app. DNAgent never
modifies a source file in place; it always writes to a new path.

## Build from source

Needed for Intel Macs, Windows, Linux, or to develop DNAgent. Requires
[Rust](https://rustup.rs) 1.92+, and for the app also Node 22+ and the [Tauri
prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
git clone https://github.com/HocheggerLab/DNAgent.git
cd DNAgent

# Command-line tool (headless, no GUI dependencies)
cargo build -p dnagent-cli --release --locked
./target/release/dnagent inspect fixtures/formats/snapgene/pUC19_M77789.dna

# Desktop app, run from source
cd desktop && npm ci && npm run tauri dev

# Desktop app, packaged for your platform
npm run tauri build
```

A locally built app has no Gatekeeper problem at all: nothing was downloaded, so nothing is
quarantined.

`npm run tauri build` can exit 0 even when bundling failed — check that
`desktop/src-tauri/target/release/bundle/` actually contains what you expect.

## Something went wrong

| Symptom | Cause | Fix |
|---|---|---|
| "DNAgent is damaged and can't be opened" | downloaded in a browser, so quarantined | `xattr -dr com.apple.quarantine /Applications/DNAgent.app` |
| "command not found: dnagent" | `/usr/local/bin` not on `PATH` | see [Install](#install) |
| `tar: could not chdir to '/usr/local/bin'` | the directory does not exist | `sudo mkdir -p /usr/local/bin` then re-run |
| The agent says no window is connected | the app is not running, or was started before the CLI was installed | start DNAgent, then retry |
| "bad CPU type in executable" | Intel Mac | [build from source](#build-from-source) |
| SmartScreen blocks the installer | unsigned on Windows | **More info → Run anyway** |
| `dnagent mcp` says it needs Unix | Windows has no unix socket | use the file handoff |

Still stuck, or something behaved unscientifically? [Open an
issue](https://github.com/HocheggerLab/DNAgent/issues) — include the construct if you can
share it, and the exact command.
