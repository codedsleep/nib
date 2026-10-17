# codefence — fenced code in the editor is highlighted in the language its
# fence names. Notes without one keep GtkSourceView's Markdown; the first note
# that names a fence's language switches every Markdown tab to nib-markdown.
source "$ROOT/scripts/agent/checks/lib.sh"
language() { q '.tabs[] | select(.active) | .language'; }
open_note folder00/note00000.md
settle 0.8
check "a note without fences keeps plain Markdown" test "$(language)" = markdown
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
