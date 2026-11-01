# EC2 runners

A push to main runs the native job, the Rust jobs and the release builds on EC2 spot instances, account
532818875494. The `ec2` job in `ci.yml` starts one instance per job with `launch.sh`. Each instance registers
a just-in-time runner with a random label, takes that one job and terminates. `cleanup.sh` terminates
whatever a cancelled run left behind. Pull requests, and any launch that fails, run on the hosted runners.

`launch.sh` tries the regions in `regions` in order: ap-south-2 (Hyderabad, the cheapest), ap-south-1
(Mumbai), ap-southeast-5 (Malaysia), ap-southeast-2 (Sydney). Each has a 300 vCPU spot limit; a run takes
128, so a region that is full passes the launch to the next.

Every instance powers off after its job, 15 minutes after boot without one, and 90 minutes after boot in any
case. The `swofty-ci-reaper` Lambda (`reaper.py`, every 10 minutes from ap-south-2) terminates any runner
older than 100 minutes and any bake instance older than two hours, in every region.

| Image | Used by | Instance |
|---|---|---|
| `linux-2604` | native, Rust (ubuntu-latest, which is 26.04) | m8a.8xlarge, 32 cores |
| `linux-2204` | release Linux build (glibc baseline) | m8a.8xlarge |
| `windows` | Rust (windows-latest), release Windows build | m8a.4xlarge, 16 cores |

Re-run a failed run with **Re-run all jobs** (`gh run rerun <id>`), not "Re-run failed jobs" (`--failed`).
A failed job's rerun keeps the label of its first runner, which has terminated, and waits in the queue for
it until GitHub gives up after a day.

The images use the hosted runners' paths (`/home/runner/work`, `/home/runner/.cargo`, `D:\a`,
`C:\Users\runneradmin\.cargo`), so Rust caches saved on either kind restore on the other.

## Rebaking an image

```sh
.github/runners/bake.sh linux-2604   # or linux-2204, windows
```

`bake.sh` boots the stock Ubuntu or Windows Server image in the first region of `regions` with
`bake-linux.sh` or `bake-windows.ps1` as user data, images the instance when it powers off, keeps the two
newest images of that kind and copies the new one to the other regions with `copy.sh`. `launch.sh` takes the
newest in each region. Rebake after a `rust-toolchain.toml` bump to keep the toolchain
preinstalled, and every few months for a current runner.

## AWS resources

- OIDC provider `token.actions.githubusercontent.com`.
- Role `swofty-launcher-ci-runners`, assumable from main and `v*` tags only. It can start, tag and
  terminate instances tagged `swofty-ci=runner` in the four regions and read the token below.
- Parameter `/swofty-ci/github-token` in ap-south-2 (SecureString): a token with admin rights on this
  repository, used to register the runners.
- Security group `swofty-ci-runners` in each region's default VPC: no inbound rules.
- Instance profile `swofty-ci-bake`, for SSM on bake instances only.
- Lambda `swofty-ci-reaper` in ap-south-2 (role `swofty-ci-reaper`, EventBridge rule `swofty-ci-reaper`),
  with the regions in its `REGIONS` variable.
