# Submodules

本仓库包含两个 Git submodule：

- `fred/`
- `redisson/`

普通执行：

```bash
git clone git@github.com:tpxxwz/redisson-rs.git
```

只会拉下主仓库，不会自动拉下 submodule 内容。

clone 后需要继续执行：

```bash
git submodule update --init --recursive
```

也可以直接使用：

```bash
git clone --recurse-submodules git@github.com:tpxxwz/redisson-rs.git
```
