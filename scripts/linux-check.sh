#!/usr/bin/env bash
# Run the Rust checks on Linux, from macOS, in a container.
#
# Three of this project's bugs have been Linux-only and invisible here: a relay that
# stalls (issue #1), lints that differ by toolchain, and `#[cfg]`-gated code that stops
# compiling on the platform it was gated away from. CI found all three, at ten minutes a
# round trip. This is the same check in about one.
#
# Usage:
#   scripts/linux-check.sh                  # clippy + tests, the CI Linux job
#   scripts/linux-check.sh test -p dnagent-cli --test mcp_relay -- --nocapture
#   scripts/linux-check.sh bash             # a shell in the container
#
# The cargo registry and a Linux-only target directory live in named volumes, so the
# first run compiles everything and later runs do not, and target/ for macOS is never
# touched by the container.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="dnagent-linux-check"
# Pinned so a toolchain change is a visible commit, not a surprise on someone's machine.
rust_version="${DNAGENT_LINUX_RUST:-1.99}"

if ! docker info >/dev/null 2>&1; then
  echo "docker is not running — start Docker Desktop (open -a Docker) and retry" >&2
  exit 1
fi

if ! docker image inspect "$image" >/dev/null 2>&1 || [ "${REBUILD:-}" = 1 ]; then
  echo "building $image (rust $rust_version)…" >&2
  docker build -t "$image" -f - "$repo" <<DOCKERFILE >&2
FROM rust:${rust_version}
# libsqlite3-sys builds its bundled C; pkg-config and a linker are already in the image.
RUN rustup component add clippy rustfmt
WORKDIR /work
DOCKERFILE
fi

args=("$@")
if [ ${#args[@]} -eq 0 ]; then
  args=(bash -c 'cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked')
elif [ "${args[0]}" = bash ]; then
  :
else
  args=(cargo "${args[@]}")
fi

exec docker run --rm ${DOCKER_TTY:--i} \
  -v "$repo:/work" \
  -v dnagent-linux-cargo:/usr/local/cargo/registry \
  -v dnagent-linux-target:/work/target-linux \
  -e CARGO_TARGET_DIR=/work/target-linux \
  -e CARGO_TERM_COLOR=always \
  "$image" "${args[@]}"
