# EC2 runners

A push to main runs the native job, the Rust jobs and the release builds on EC2 spot instances in
ap-south-2 (Hyderabad), account 532818875494. The `ec2` job in `ci.yml` starts one instance per job with
`launch.sh`. Each instance registers a just-in-time runner with a random label, takes that one job and
terminates. `cleanup.sh` terminates whatever a cancelled run left behind. Pull requests, and any launch
that fails, run on the hosted runners.

| Image | Used by | Instance |
|---|---|---|
| `linux-2404` | native, Rust (ubuntu-latest) | m8a.8xlarge, 32 cores |
| `linux-2204` | release Linux build (glibc baseline) | m8a.8xlarge |
| `windows` | Rust (windows-latest), release Windows build | m8a.4xlarge, 16 cores |

The images use the hosted runners' paths (`/home/runner/work`, `/home/runner/.cargo`, `D:\a`,
`C:\Users\runneradmin\.cargo`), so Rust caches saved on either kind restore on the other.

## Rebaking an image

```sh
.github/runners/bake.sh linux-2404   # or linux-2204, windows
```

`bake.sh` boots the stock Ubuntu or Windows Server image with `bake-linux.sh` or `bake-windows.ps1` as
user data, images the instance when it powers off, and keeps the two newest images of that kind.
`launch.sh` takes the newest. Rebake after a `rust-toolchain.toml` bump to keep the toolchain
preinstalled, and every few months for a current runner.

## AWS resources

- OIDC provider `token.actions.githubusercontent.com`.
- Role `swofty-launcher-ci-runners`, assumable from main and `v*` tags only. It can start, tag and
  terminate instances tagged `swofty-ci=runner` and read the token below.
- Parameter `/swofty-ci/github-token` (SecureString): a token with admin rights on this repository,
  used to register the runners.
- Security group `swofty-ci-runners`: no inbound rules.
- Instance profile `swofty-ci-bake`, for SSM on bake instances only.
