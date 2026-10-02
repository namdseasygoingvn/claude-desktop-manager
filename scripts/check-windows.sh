#!/bin/sh
# cargo check never links, so empty objects satisfy the C build scripts (zstd-sys) and the resource compiler (tauri-winres -> embed-resource).
set -eu

TARGET=x86_64-pc-windows-msvc

if ! rustup target list --installed | grep -qx "$TARGET"; then
  echo "check-windows: rust target $TARGET is not installed. Run: rustup target add $TARGET" >&2
  exit 1
fi

ROOT=$(cd "$(dirname "$0")/.." && pwd)
STUBS=$(mktemp -d)
mkdir "$STUBS/bin"
trap 'rm -rf "$STUBS"' EXIT INT TERM

cat > "$STUBS/cc" <<'EOF'
#!/bin/sh
prev=
for a in "$@"; do
  case "$a" in
    -Fo*) : > "${a#-Fo}" ;;
    -out:*) : > "${a#-out:}" ;;
    /OUT:*) : > "${a#/OUT:}" ;;
    *) if [ "$prev" = -o ]; then : > "$a"; fi ;;
  esac
  prev=$a
done
exit 0
EOF
cp "$STUBS/cc" "$STUBS/ar"

cat > "$STUBS/bin/llvm-rc" <<'EOF'
#!/bin/sh
prev=
for a in "$@"; do
  case "$a" in
    *'/?'*) echo "OVERVIEW: LLVM Resource Converter no-preprocess"; exit 0 ;;
  esac
  if [ "$prev" = /fo ]; then : > "$a"; fi
  prev=$a
done
exit 0
EOF

chmod +x "$STUBS/cc" "$STUBS/ar" "$STUBS/bin/llvm-rc"

cd "$ROOT/src-tauri"
env PATH="$STUBS/bin:$PATH" \
  CC_x86_64_pc_windows_msvc="$STUBS/cc" \
  AR_x86_64_pc_windows_msvc="$STUBS/ar" \
  cargo check --target "$TARGET" --target-dir target/windows-check --message-format short "$@"
