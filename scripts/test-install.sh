#!/usr/bin/env bash
# Exercise real packaging and both installers in a disposable prefix.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
archive=$(scripts/package.sh)
mkdir "$tmp/payload" "$tmp/mock" "$tmp/download"
tar -xzf "$archive" -C "$tmp/payload"
prefix="$tmp/prefix with spaces"
scripts/install-files.sh -y --prefix "$prefix" --payload "$tmp/payload"
[[ $("$prefix/bin/nib" --version) == "$(target/release/nib --version)" ]]
[[ -s $prefix/share/man/man1/nib.1 && -s $prefix/share/gtksourceview-5/styles/nib-nord.xml ]]
# An unrelated program, config and vault data must survive uninstall.
printf 'keep' >"$prefix/bin/unrelated"
scripts/install.sh -y --prefix "$prefix" --uninstall
[[ $(cat "$prefix/bin/unrelated") == keep && ! -e $prefix/bin/nib ]]
rm "$prefix/bin/unrelated"
rmdir "$prefix/bin" "$prefix"
cp "$archive" "$tmp/download/"
(cd "$tmp/download"; sha256sum nib-*.tar.gz > SHA256SUMS)
cat >"$tmp/mock/gh" <<'MOCK'
#!/usr/bin/env bash
set -eu
if [[ $1 == auth && $2 == status ]]; then exit 0; fi
[[ $1 == release && $2 == download ]]
while (($#)); do
    if [[ $1 == --dir ]]; then dest=$2; break; fi
    shift
done
cp "$NIB_INSTALL_TEST_DOWNLOAD"/* "$dest/"
MOCK
chmod +x "$tmp/mock/gh"
export NIB_INSTALL_TEST_DOWNLOAD="$tmp/download"
PATH="$tmp/mock:$PATH" ./install.sh -y --prefix "$prefix"
[[ -x $prefix/bin/nib ]]
./install.sh -y --prefix "$prefix" --uninstall
[[ ! -e $prefix ]]
# A corrupted archive must be rejected before any destination is written.
printf 'tamper' >>"$tmp/download/${archive##*/}"
if PATH="$tmp/mock:$PATH" ./install.sh -y --prefix "$prefix" >"$tmp/rejected" 2>&1; then
    echo 'FAIL: installer accepted a corrupt archive' >&2; exit 1
fi
[[ ! -e $prefix ]]
if ./install.sh --prefix "$prefix" </dev/null >"$tmp/noninteractive" 2>&1; then
    echo 'FAIL: installer did not require -y without a terminal' >&2; exit 1
fi
[[ ! -e $prefix ]]
python3 "$root/scripts/generate-docs.py" --check
echo 'Installer checks: payload, custom prefix, checksum refusal and uninstall passed'
