[English](DISTRIBUTION.md) | **中文**

# 分发渠道

一次 tag push 驱动所有渠道。GitHub Release 仍然是二进制文件和校验和的唯一事实
来源。

| 渠道 | 平台 | 命令 | 状态 |
|---------|----------|---------|--------|
| [Homebrew](#homebrew) | macOS、Linux | `brew install LiangLliu/tap/password-cracking` | 已上线，已针对 v0.2.1 验证 |
| [APT](#debian--ubuntu-apt) | Debian、Ubuntu、Raspberry Pi OS | `sudo apt install password-cracking` | 已上线，已签名，已验证 |
| [Docker](#docker-ghcr) | Linux 容器 | `docker run ghcr.io/lianglliu/password-cracking` | 已上线，amd64 + arm64 |
| [AUR](#arch-linux-aur) | Arch Linux | `yay -S password-cracking-bin` | PKGBUILD 已生成；提交待处理 |
| [crates.io](#cratesio) | 任何 Rust 平台 | `cargo install password-cracking` | 流水线就绪；需要 token |
| [install.sh](#installsh) | Linux、macOS | `curl …/install.sh \| sh` | 已上线，已验证 |
| [GitHub Releases](https://github.com/LiangLliu/password-cracking/releases) | Linux、macOS | 直接下载 | 39 个产物 + `SHA256SUMS` |

---

## Homebrew

```bash
brew install LiangLliu/tap/password-cracking
brew upgrade password-cracking
```

formula 位于
[`LiangLliu/homebrew-tap`](https://github.com/LiangLliu/homebrew-tap)，并在每次
发布时自动更新。macOS 使用原生构建；Linux 使用静态 musl 构建，因此 formula 与
发行版的 glibc 版本无关。

formula 会安装二进制文件、man page 以及 bash/zsh/fish 补全。

已在 macOS arm64 上验证：`brew install`、`brew test`、补全、man page 以及一次
真实的 fixture 破解。

## Debian / Ubuntu (APT)

仓库托管在 GitHub Pages 上，并使用项目密钥签名。

```bash
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null

echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] \
https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list

sudo apt update
sudo apt install password-cracking
```

签名密钥指纹：

```text
17E4 9D06 F4E3 B63C 8868  EC40 C334 9BA9 9EBE 5A92
```

使用以下命令验证：

```bash
gpg --show-keys /etc/apt/keyrings/password-cracking.gpg
```

支持的架构：`amd64`、`arm64`、`armhf`（Raspberry Pi 2+）。

版本锁定（pinning）照常工作：

```bash
sudo apt install password-cracking=0.2.1
# Downgrading an installed version also needs --allow-downgrades
```

仓库会保留多个版本（`dpkg-scanpackages --multiversion`），因此版本锁定和可复现
安装始终可用。每个架构每次发布大约占用 700 KB；GitHub Pages 的限制（1 GB）在
可预见的未来都不是问题。

已在 Debian 容器中针对线上 URL 验证：`apt update`、签名检查、`apt install` 以及
`password-cracking --version`。

## Docker (GHCR)

```bash
docker pull ghcr.io/lianglliu/password-cracking:latest

docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking \
  -f /data/document.zip dictionary -w /data/wordlist.txt
```

- 多架构：`linux/amd64`、`linux/arm64`
- 围绕静态 musl 二进制以 `FROM scratch` 构建；镜像约 1.7 MB
- 标签：`latest`、`0.2`、`0.2.1`
- 没有 shell、没有 libc、没有网络访问；挂载目标文件和词表
- 容器以 root 运行，因此全局可读的挂载无需额外参数即可工作。如果词表访问受限，
  传入 `--user "$(id -u):$(id -g)"`。

已使用 `docker run … --version` 对带版本标签和 `latest` 标签分别验证。

## Arch Linux (AUR)

包名为 `password-cracking-bin`。

```bash
yay -S password-cracking-bin
# or
paru -S password-cracking-bin
```

发布 job 会生成 `PKGBUILD` 和 `PKGBUILD.SRCINFO`，并将它们附加到发布中（AUR
检出时会把后者安装为 `.SRCINFO`）。配置了 `AUR_SSH_PRIVATE_KEY` secret 后，它还
会推送到 `ssh://aur@aur.archlinux.org/password-cracking-bin.git`。

在首次自动推送之前，包必须已存在于 AUR 中：

1. 从发布中下载 `PKGBUILD` 和 `PKGBUILD.SRCINFO`；将后者重命名为 `.SRCINFO`。
2. 在 <https://aur.archlinux.org/submit/> 提交 `PKGBUILD`。
3. 将 CI 密钥的公钥部分添加到 AUR 账户
   （**My Account → SSH Public Key**）。
4. 将私钥存储为 `AUR_SSH_PRIVATE_KEY` secret。

支持的架构：`x86_64`、`aarch64`。PKGBUILD 使用 gnu tarball，并在打包时生成 man
page 和补全。

## crates.io

```bash
cargo install password-cracking
# until the crate is published:
cargo install --git https://github.com/LiangLliu/password-cracking
```

当配置了 `CARGO_REGISTRY_TOKEN` 且版本尚未出现在 crates.io 上时，会自动发布。
该 job 会将 `Cargo.toml` 与 tag 比较，并跳过已发布的版本。在
<https://crates.io/settings/tokens> 创建 token，然后：

```bash
gh secret set CARGO_REGISTRY_TOKEN
gh workflow run publish.yml -f version=0.2.1
```

crate 名称 `password-cracking` 目前可用。

## install.sh

```bash
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

检测操作系统和架构，在 Linux 上优先使用静态 musl 构建，校验发布的 SHA-256，并
安装到 `/usr/local` 或 `~/.local`，无需 `sudo`。

## GitHub Releases

每个发布包含：

- `password-cracking-linux-{amd64,arm64,armv7}{,-musl}.tar.gz`
- `password-cracking-macos-{amd64,arm64}.tar.gz`
- `.deb` 用于 `amd64`、`arm64`、`armhf`
- `.rpm` 用于 `x86_64`、`aarch64`、`armv7hl`
- `.apk` 用于 `x86_64`、`aarch64`、`armv7`
- `install.sh`、`SHA256SUMS`、Homebrew formula、`PKGBUILD`、`PKGBUILD.SRCINFO`

每个归档/包都有一个 `.sha256` 文件。使用以下命令校验所有内容：

```bash
gh release download v0.2.1 --dir dist
cd dist && sha256sum -c SHA256SUMS
```

---

## 发布机制

```text
git tag v0.2.1 && git push origin v0.2.1
        │
        ▼
CI (ci.yml)
  quality / audit / tests
        │
        ▼
build matrix (8 targets) ──► GitHub Release
        │                     tarballs, deb/rpm/apk, checksums
        ▼
publish (publish.yml, reusable)
  ├── apt       build a signed repo from .deb assets, push gh-pages
  ├── homebrew  render formula from checksums, push homebrew-tap
  ├── aur       render PKGBUILD/.SRCINFO, optional AUR push
  ├── docker    multi-arch image to ghcr.io
  └── crates    cargo publish (optional, needs token)
```

发布工作流可以对任何已有的发布重新运行：

```bash
gh workflow run publish.yml -f version=0.2.1
```

它从 GitHub Release 下载产物，因此重新运行不会重新构建二进制文件。未配置凭据的
渠道会被跳过并给出提示，而不是让发布失败。

### 清单文件

`scripts/gen-package-manifests.py` 会根据一个存放发布产物及其 `.sha256` 文件的
目录，渲染每个 manifest：

```bash
gh release download v0.2.1 --dir dist
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.1 --out manifests
```

当某个产物缺失时，它会拒绝生成 manifest。模板位于 `packaging/`。

---

## 一次性设置

这些步骤已在 `LiangLliu/password-cracking` 中完成；此处记录是为了 fork 和密钥
轮换。

### 1. APT 签名密钥

生成一个 RSA 4096 签名密钥。CI 会无人值守地使用它，因此它没有口令：

```bash
export GNUPGHOME=$(mktemp -d)
gpg --batch --generate-key <<'EOF'
%echo generating
Key-Type: RSA
Key-Length: 4096
Key-Usage: sign
Name-Real: password-cracking APT repository
Name-Email: you@example.com
Expire-Date: 0
%no-protection
%commit
EOF

gpg --batch --export-secret-keys > /tmp/apt-private.asc
gpg --batch --export > packaging/apt/password-cracking.gpg
gh secret set APT_GPG_PRIVATE_KEY < /tmp/apt-private.asc
shred -u /tmp/apt-private.asc && rm -rf "$GNUPGHOME"
```

提交 `packaging/apt/password-cracking.gpg`；发布 job 会把它复制到站点根目录并
命名为 `key.gpg`。**轮换密钥会破坏现有用户的配置** —— 公布新的指纹，并在过渡
期内保留旧密钥可用。

### 2. GitHub Pages

APT 仓库从 `gh-pages` 分支提供：

```bash
gh api -X POST repos/LiangLliu/password-cracking/pages \
  --input - <<< '{"source":{"branch":"gh-pages","path":"/"}}'
```

发布工作流会在需要时启用 Pages 并请求构建。站点为
<https://lianglliu.github.io/password-cracking/>。

### 3. Homebrew tap

1. 创建 `LiangLliu/homebrew-tap`（公开）。
2. 添加一个具有写权限的部署密钥：

```bash
ssh-keygen -t ed25519 -N '' -f tap_key
gh repo deploy-key add tap_key.pub --repo LiangLliu/homebrew-tap \
  --title "password-cracking release" --allow-write
gh secret set HOMEBREW_TAP_DEPLOY_KEY < tap_key
rm -f tap_key tap_key.pub
```

### 4. AUR

创建一次包（参见 [AUR](#arch-linux-aur)），然后：

```bash
gh secret set AUR_SSH_PRIVATE_KEY
```

### 5. crates.io

```bash
cargo login <token>                       # https://crates.io/settings/tokens
gh secret set CARGO_REGISTRY_TOKEN
```

### 6. 容器可见性

由 `GITHUB_TOKEN` 创建的 GHCR 包对公开仓库来说是公开的，但请在首次推送后验证
一次：

```bash
docker pull ghcr.io/lianglliu/password-cracking:latest
```

如果拉取未授权，打开
**Packages → password-cracking → Package settings → Change visibility → Public**。

---

## 验证清单

```bash
# 1. APT (live repository)
docker run --rm debian:stable-slim bash -c '
  apt-get update -qq &&
  apt-get install -y -qq curl gnupg ca-certificates &&
  install -m 0755 -d /etc/apt/keyrings &&
  curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
    -o /etc/apt/keyrings/password-cracking.gpg &&
  echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] \
https://lianglliu.github.io/password-cracking stable main" \
    > /etc/apt/sources.list.d/pc.list &&
  apt-get update -qq && apt-get install -y -qq password-cracking &&
  password-cracking --version'

# 2. Homebrew
brew install LiangLliu/tap/password-cracking
brew test LiangLliu/tap/password-cracking
brew uninstall password-cracking

# 3. Docker
docker run --rm ghcr.io/lianglliu/password-cracking --version

# 4. Installer
./scripts/install.sh --prefix /tmp/pc

# 5. Manifests
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.1 --out /tmp/m
bash -n /tmp/m/PKGBUILD
ruby -c /tmp/m/password-cracking.rb
```

仓库元数据：

```bash
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/InRelease \
  | gpg --verify
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/Release
```

---

## 排错

| 现象 | 原因 / 修复 |
|---------|-------------|
| `apt update`：`NO_PUBKEY` | `signed-by` 路径错误，或密钥已轮换。重新下载 `key.gpg`。 |
| `apt update`：`403` | Pages 构建仍在运行；等待一分钟后重试。 |
| `brew`：`No available formula` | tap 推送失败。检查 `homebrew` job 和部署密钥。 |
| `brew`：`SHA256 mismatch` | formula 渲染后产物被替换。重新运行 `publish.yml`。 |
| GHCR `unauthorized` | 包是私有的；更改其可见性。 |
| `cargo publish`：`already exists` | 符合预期；该 job 会跳过已发布的版本。先提升 `Cargo.toml` 中的版本。 |
| AUR 推送失败 | AUR 包尚未存在，或 SSH 密钥未注册。PKGBUILD 仍会附加到发布中。 |
| `nfpm`：ARMv7 架构错误 | Debian 使用 `armhf`，RPM 使用 `armv7hl`，Alpine 使用 `armv7`；CI 会按打包器分别映射。 |
| `brew install` 失败并提示 `Path contains '..'` | macOS 归档是使用父路径构建的。当前 CI 归档使用扁平名称；重新构建该发布。 |

---

## 添加另一个渠道

`packaging/` 以 manifest 为先：

```text
packaging/
  apt/index.html               landing page for the APT site
  apt/password-cracking.gpg    public signing key
  aur/PKGBUILD.tmpl
  aur/.SRCINFO.tmpl
  homebrew/password-cracking.rb.tmpl
  nfpm.yaml                    deb / rpm / apk
```

要添加一个渠道（例如 Nix 或 Snap）：

1. 在 `packaging/<channel>/` 下添加模板。
2. 用新的输出扩展 `scripts/gen-package-manifests.py`。
3. 向 `.github/workflows/publish.yml` 添加一个 job。
4. 使用 `python3 scripts/gen-package-manifests.py …` 在本地验证。
