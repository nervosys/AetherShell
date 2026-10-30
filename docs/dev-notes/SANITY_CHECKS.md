# IronShell Shell - Sanity Checks

```shell
# ish> [1,2,3]
# Array([Int(1), Int(2), Int(3)])

# ish> [1,2,3,4] | map fn(x)=> x*x | reduce fn(a,b)=> a+b 0
# Int(30)

# ish> [5,4,3,2,1] | where fn(x)=> x>2 | take 2
# Array([Int(5), Int(4)])

# ish> [1,2,3] | fn(x)=> x*2
# Array([Int(2), Int(4), Int(6)])

# ish> {name:"a", size:42}
# Record({"name": Str("a"), "size": Int(42)})

# ish> print("hi")
# "hi"
# Str("hi")
# ish> print "hi"
# "hi"
# Str("hi")
# ish> "hi" | print
# "hi"
# Str("hi")

# ish> fn(x)=> x*x
# Lambda(Lambda { params: ["x"], body: Binary { left: Ident("x"), op: Mul, right: Ident("x") } })
```
