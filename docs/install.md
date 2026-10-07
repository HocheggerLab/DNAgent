# Installing DNAgent

Current builds are **macOS on Apple Silicon** (M1 and later). Intel Macs, Windows and
Linux are not built yet; on those, [build from source](#build-from-source).

DNAgent comes in two parts, and they are useful separately:

- the **desktop app**, for looking at and editing constructs;
- the **`dnagent` command-line tool**, which does the biology and is what an AI agent
  drives.

Install both if you want the agent workflows; the app alone is enough to open plasmids.

## Install

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

## Connect an agent

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

## Updating

Re-run the install command — `tar` overwrites in place. Quit DNAgent first if it is
running.

## Uninstalling

```bash
rm -rf /Applications/DNAgent.app /usr/local/bin/dnagent
rm -rf ~/Library/Application\ Support/uk.ac.sussex.hochegger.dnagent   # settings
```

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

Still stuck, or something behaved unscientifically? [Open an
issue](https://github.com/HocheggerLab/DNAgent/issues) — include the construct if you can
share it, and the exact command.
