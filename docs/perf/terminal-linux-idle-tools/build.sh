#!/bin/bash
# build.sh <outname>: frontend + release atlas-desktop (debug-assertions for the bridge) -> bin/<outname>
cd /home/matheus/Projects/atlas-wt/perf/term-linux-idle-wakeups
export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/home/matheus/.cache/atlas-agents-target-gpu CARGO_BUILD_BUILD_DIR=/home/matheus/.cache/atlas-agents-target-gpu/build
(cd apps/desktop && mise exec bun@latest -- bun run build >/dev/null 2>&1) || { echo "frontend failed"; exit 1; }
scripts/heavy.sh cargo build --release --locked -p atlas-desktop -p atlas-cli --features tauri/custom-protocol --config profile.release.package.atlas-desktop.debug-assertions=true 2>&1 | tail -3 || exit 1
cp $CARGO_TARGET_DIR/release/atlas-desktop /home/matheus/.cache/GpuTermIdle/bin/$1
cp $CARGO_TARGET_DIR/release/atlas /home/matheus/.cache/GpuTermIdle/bin/atlas-$1
