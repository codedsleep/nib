# listcont — editor behaviours in vim insert mode (6.3): list continuation
# (bullets, numbers, empty marker removed), Tab/Shift+Tab on a list line,
# Ctrl+Enter checkbox, auto-pair of [[, Ctrl+Shift+Up/Down headings.
# run-all.sh runs it twice: English, and German UI strings (G3; de_DE is not
# installed here, so LANGUAGE=de on en_GB.UTF-8).
source "$ROOT/scripts/agent/checks/lib.sh"
open_note linker01.md
focus_editor
vin key G key o
settle 0.2
# GtkSourceView's vim bar is untranslated in apps without a text domain, so
# either form may show; the behaviours below must work regardless.
check "insert mode shows the insert bar" has "$(q .status.mode)" '^-- (INSERT|EINFÜGEN) --$'
vin type "- one" key Return type two key Return key Return
vin type "1. a" key Return type b key Return key Tab key shift+Tab type c key Escape
vin key ctrl+Return key ctrl+Return
vin key o type "see [[" type hub key Escape key Escape
vin key o type "## Head" key Escape
vin key ctrl+shift+Up key ctrl+shift+Down key ctrl+shift+Down
vin type :w key Return
settle 0.5
expected=$'- one\n- two\n1. a\n2. b\n3. [x] c\nsee [[hub]]\n### Head'
got=$(tail -n 7 "$VAULT/linker01.md")
check "behaviours wrote the expected lines" test "$got" = "$expected"
[[ $got == "$expected" ]] || printf 'got:\n%s\n' "$got"
