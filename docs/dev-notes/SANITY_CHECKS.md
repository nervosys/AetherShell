# Aether Shell - Sanity Checks

```shell
# ae> [1,2,3]
# Array([Int(1), Int(2), Int(3)])

# ae> [1,2,3,4] | map fn(x)=> x*x | reduce fn(a,b)=> a+b 0
# Int(30)

# ae> [5,4,3,2,1] | where fn(x)=> x>2 | take 2
# Array([Int(5), Int(4)])

# ae> [1,2,3] | fn(x)=> x*2
# Array([Int(2), Int(4), Int(6)])

# ae> {name:"a", size:42}
# Record({"name": Str("a"), "size": Int(42)})

# ae> print("hi")
# "hi"
# Str("hi")
# ae> print "hi"
# "hi"
# Str("hi")
# ae> "hi" | print
# "hi"
# Str("hi")

# ae> fn(x)=> x*x
# Lambda(Lambda { params: ["x"], body: Binary { left: Ident("x"), op: Mul, right: Ident("x") } })
```
