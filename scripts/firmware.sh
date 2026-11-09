#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"

# espup emits these settings; allow a caller-supplied export path.
esp_export=${ESP_EXPORT_FILE:-"$HOME/export-esp.sh"}
if [ -f "$esp_export" ]; then
    . "$esp_export"
fi

# ESP-IDF resolves custom partition paths relative to its generated CMake project.
# Generate an absolute path locally so the checked-in defaults stay relocatable.
mkdir -p target
python3 - "$task_root" <<'PY'
from pathlib import Path
import hashlib
import sys
root = Path(sys.argv[1])
text = (root / 'firmware/sdkconfig.defaults').read_text()
text = text.replace('"firmware/partitions.csv"', '"' + (root / 'firmware/partitions.csv').as_posix() + '"')
# Cargo cannot otherwise see edits inside ESP-IDF's native component. Touching
# this tracked defaults input makes esp-idf-sys rerun CMake/Ninja when needed.
native = root / 'firmware/components/pdi'
digest = hashlib.sha256()
for path in sorted(p for p in native.rglob('*') if p.is_file()):
    digest.update(path.relative_to(native).as_posix().encode())
    digest.update(path.read_bytes())
text += '\n# Native component SHA256: ' + digest.hexdigest() + '\n'
out = root / 'target/badge-sdkconfig.defaults'
if not out.exists() or out.read_text() != text:
    out.write_text(text)
PY
ESP_IDF_SDKCONFIG_DEFAULTS="$task_root/target/badge-sdkconfig.defaults"
export ESP_IDF_SDKCONFIG_DEFAULTS

action=${1:-build}
if [ "$#" -gt 0 ]; then shift; fi
case "$action" in
    build) cargo +esp build --locked -p badge-apple --release --target xtensa-esp32s3-espidf "$@" ;;
    flash|image)
        cargo +esp build --locked -p badge-apple --release --target xtensa-esp32s3-espidf
        task_bootloader=$(python3 - <<'PY'
from pathlib import Path
files = list(Path('target/xtensa-esp32s3-espidf/release/build').glob('esp-idf-sys-*/out/build/bootloader/bootloader.bin'))
if not files:
    raise SystemExit('ESP-IDF bootloader binary not found')
print(max(files, key=lambda p: p.stat().st_mtime))
PY
        )
        task_elf=target/xtensa-esp32s3-espidf/release/badge-apple
        if [ "$action" = flash ]; then
            espflash flash --monitor --flash-size 8mb --flash-mode dio --flash-freq 80mhz \
                --partition-table firmware/partitions.csv --bootloader "$task_bootloader" "$task_elf" "$@"
        else
            espflash save-image --chip esp32s3 --merge --skip-padding --flash-size 8mb \
                --flash-mode dio --flash-freq 80mhz --partition-table firmware/partitions.csv \
                --bootloader "$task_bootloader" "$task_elf" target/badge-apple-esp32s3.bin "$@"
        fi
        ;;
    check) cargo +esp clippy --locked -p badge-apple --release --target xtensa-esp32s3-espidf -- -D warnings ;;
    *) echo "usage: $0 {build|flash|image|check} [extra arguments]" >&2; exit 2 ;;
esac
