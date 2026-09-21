<!-- FROZEN for E7. Extracted from AGENTS.md's Option 5 section: the v2
     table, v5 features, the builtin shorthand map, and the symbol/v4
     mappings. It deliberately EXCLUDES the function-abbreviation map
     (~200 entries) and the module sigil map (92 entries), which is what
     the docs mean by "the full mapping in AGENTS.md is larger still".
     Do not edit: E7 hashes this directory. -->

# AetherShell agentic syntax — quick reference

**Ultra-compressed syntax (v2) — maximum density:**

| Ultra (v2)            | v1 Compat      | AetherShell                        | Savings  |
| --------------------- | -------------- | ---------------------------------- | -------- |
| `x=42`                | `x=42`         | `let x = 42`                       | 2 tokens |
| `x:=0`                | `x:=0`         | `let mut x = 0`                    | 3 tokens |
| `~x:x*2`              | `\x:x*2`       | `fn(x) => x * 2`                   | 4 tokens |
| `~.size>1k`           | `\.size>1k`    | `fn(__) => __.size > 1000`         | 6 tokens |
| `a\|b`                | `a > b`        | `a \| b`                           | same     |
| `F.r("p")`            | `@f.r("p")`    | `file.read("p")`                   | 2 tokens |
| `S.h()`               | `@s.h()`       | `sys.hostname()`                   | 2 tokens |
| `H.g(url)`            | `@h.g(url)`    | `http.get(url)`                    | 2 tokens |
| `DK.p()`              | `@dk.ps()`     | `docker.ps()`                      | 2 tokens |
| `e"msg"`              | `#e "msg"`     | `echo("msg")`                      | 2 tokens |
| `l"."`                | `#l "."`       | `ls(".")`                          | 2 tokens |
| `w~.size>1k`          | `#w \.size>1k` | `where(fn(__) => __.size > 1000)`  | 8 tokens |
| `\|w.size>1k`          | —              | `\| where(fn(__) => __.size > 1000)` | +2 tokens |
| `m~x:x*2`             | `#m \x:x*2`    | `map(fn(x) => x * 2)`              | 6 tokens |
| `t5`                  | `#t 5`         | `take(5)`                          | 2 tokens |
| `1k` / `1M` / `1G`    | same           | `1000` / `1000000` / `1000000000`  | 1 token  |
| `?val{A=>"x",_=>"z"}` | same           | `match val { A => "x", _ => "z" }` | 3 tokens |
| `!{expr}{"fb"}`       | same           | `try { expr } catch e { "fb" }`    | 5 tokens |
| `; comment`           | same           | `// comment`                       | same     |

**v5 features — auto-parens and for-each:**

| v5 Syntax             | AetherShell                            | Savings   |
| --------------------- | -------------------------------------- | --------- |
| `F.r"path"`           | `file.read("path")`                    | 2 chars   |
| `H.g"https://api.io"` | `http.get("https://api.io")`           | 2 chars   |
| `*[1,2,3]~x:echo(x)`  | `([1,2,3]) \| each(fn(x) => echo(x))`  | 10 tokens |
| `*items~i:proc(i)`    | `(items) \| each(fn(i) => proc(i))`    | 8 tokens  |
| `*arr.range(5)\n:n*n` | `(arr.range(5)) \| each(fn(n) => n*n)` | 8 tokens  |

**Builtin shorthand map (single-char → builtin, used as `#x` or bare `x` — all 26 a–z assigned):**

`a`=all `b`=flatten `c`=cat `d`=debug `e`=echo `f`=find `g`=grep `h`=head `i`=first `j`=join `k`=keys `l`=ls `m`=map `n`=len `o`=sort `p`=print `q`=reverse `r`=reduce `s`=select `t`=take `u`=uniq `v`=values `w`=where `x`=sh `y`=any `z`=last

**Symbol→value mapping (v3) — maximum density:**

`T`→true `N`→null `'text'`→`"text"` `` `cmd` ``→`sh("cmd")` `l./src`→`ls("./src")` `l/usr/bin`→`ls("/usr/bin")` `g*.rs`→`grep("*.rs")`

**Compactness + expandability (v4) — joint optimization:**

`|.name`→`| map(fn(__) => __.name)` `|.data.items`→field chains `|.trim()`→method calls `$HOME`→`sys.env("HOME")` `^cond{then}`→`match (cond) { true => (then), _ => null }` `^cond{then}{else}`→`match (cond) { true => (then), _ => (else) }` `%def name expansion`→user alias

