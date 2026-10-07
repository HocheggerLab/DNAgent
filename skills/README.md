# DNAgent agent skills

Workflow instructions for an AI agent driving the `dnagent` CLI and desktop app, kept
here rather than in a personal skills collection because they **document this tool and
version with it**. A skill pins a contract — the CLI output envelope, the assembly-plan
schema — and when those change, the skill changes in the same commit. In a separate repo
it would drift, and an agent would promise a flag the shipped binary does not have.

| Skill | What it is for |
|---|---|
| `dnagent` | Reading, annotating and writing constructs; digests, translation, ORFs, the feature library; the desktop handoff and the live MCP session. |
| `gibson-cloning` | The full insert-into-vector Gibson recipe on top of `dnagent`: vector digest, cores, primers, the annotated product and a design record. |

## Using them

**Claude Code** reads skills from a directory:

```bash
mkdir -p ~/.claude/skills
cp -R skills/dnagent skills/gibson-cloning ~/.claude/skills/
```

**Claude Desktop / Cowork** takes one zip per skill, uploaded at
Settings → Capabilities:

```bash
(cd skills/dnagent && zip -qr ../../dnagent.skill.zip .)
(cd skills/gibson-cloning && zip -qr ../../gibson-cloning.skill.zip .)
```

Both need the `dnagent` binary on `PATH` — see [docs/install.md](../docs/install.md).
The skills run commands; they do not reimplement any biology.

## Checking them

Each skill carries a smoke check that runs its documented commands against public
fixtures in this repo:

```bash
python3 skills/dnagent/scripts/check_skill.py --repo . --binary target/debug/dnagent
python3 skills/gibson-cloning/scripts/check_skill.py --repo . --binary target/debug/dnagent
```

## For the maintainer

`~/code/skills` carries a copy for bundling and upload, refreshed from here by
`skillctl.sh sync` (see its `upstream.conf`). **This repo is the source**: edit here, in
the same commit as the code the skill describes, then sync. Editing the copy loses the
change on the next sync.
