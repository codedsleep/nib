# codefence — fenced code is highlighted in the language its fence names, in
# the editor and the reading view. Notes without one keep GtkSourceView's
# Markdown; the first note that names a fence's language switches every
# Markdown tab to nib-markdown. The reading view tags the tokens itself.
source "$ROOT/scripts/agent/checks/lib.sh"
language() { q '.tabs[] | select(.active) | .language'; }
open_note folder00/note00000.md
settle 0.8
check "a note without fences keeps plain Markdown" test "$(language)" = markdown
vin key alt+e
settle 0.5
check "a note without fences has no code tokens in reading view" test "$(q .reading.tokens)" = 0
vin key alt+e
settle 0.3
shot prose
cat >"$VAULT/code.md" <<'NOTE'
# Code

```rust
fn main() {
    let greeting = "hello"; // a comment
    println!("{greeting}");
}
```

~~~python
def square(x):
    return x ** 2  # ~ and ` inside code do not end the block
~~~

```sh
echo "$HOME" `date`
```

``` ts
const greet = (name: string): string => `Hello, ${name}`;
```

```
plain block
```

````markdown
```rust
a shorter fence inside a longer one is content
```
````

## After the blocks
NOTE
open_note code.md
wait_q '.tabs[] | select(.active) | .language' nib-markdown 5
check "a named fence loads nib-markdown" test "$(language)" = nib-markdown
settle 0.5
shot code-fences
vin key alt+e
settle 0.6
check "the reading view shows the note" test "$(q .reading.mode)" = reading
check "the reading view tags code tokens" test "$(q .reading.tokens)" -gt 10
shot code-reading
