# EC2 runners

A push to main runs the native job, the Rust test jobs and the release builds on EC2 spot instances, account
532818875494. The `ec2` job in `ci.yml` starts one instance per job with `launch.sh`, only for the areas the
change touches. Each instance registers a just-in-time runner with a random label, takes that one job and
terminates. `cleanup.sh` terminates whatever a cancelled run left behind. Formatting and Clippy always run on
hosted runners. Pull requests, and any launch that fails, run on the hosted runners.

Pull requests from branches of this repository (never forks) use the instances too when the repository
variable `EC2_PULL_REQUESTS` is `true`. That needs the role below to trust the `pull_request` subject first;
until then the role refuses the request and the jobs fall back to the hosted runners.

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
preinstalled, after a change to the packages `bake-linux.sh` installs, and every few months for a current
runner. The Linux images carry every package a job asks `apt-install.sh` for, so on them the script returns
without running apt; a package missing from the image is installed at run time, which is slower and depends
on the regional archive mirror.

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

## Cache budget

The repository has 10 GB of Actions cache space, and GitHub evicts the least recently used caches beyond it.
The workflows save these, measured on hosted and EC2 runs in November 2026:

| Cache | Saved by | Size |
|---|---|---|
| `v0-rust-test-…-Linux-…` | Rust tests on main, and on a pull request whose key main lacks | 0.96 GB |
| `v0-rust-test-…-Windows_NT-…` | the same | 0.84 GB |
| `v0-rust-installer-…-Windows_NT-…` | the Windows installer job on main; pull requests only restore it | 0.25–0.35 GB, expected |
| `zig-Linux-0.16.0-<month>-<hash>` | the native job on main, on EC2 with only the files that run read; the step after it deletes main's older ones | 2.85 GB with every file; a hosted run reads 96 MB (host part) and 187 MB (Wine part) of it |
| `ccache-Linux-zig0.16.0-<part>-<run>` | the native job on every branch, one per part (`all` on EC2, `host` and `wine` on hosted runners); the step after it deletes that branch's older ones | 0.11 GB for the host part, 0.02 GB for the Wine part |
| `wine-debs-Linux-<hash>` | the hosted Wine job, when its set of Wine packages changes; the step after it deletes that branch's older ones | 0.25 GB |
| `pnpm-*` | the documentation build on main | 0.10 GB |
| `setup-zig-tarball-*` | `mlugg/setup-zig`, per branch | 0.05–0.09 GB |

Main holds about 2.4–2.8 GB with a Zig cache of only the files its run read, and 5.2–5.4 GB with every file,
which the native job keeps when the runner's file system records no reads. Every pull request that changes
native code adds its own ccaches and Wine packages (0.4 GB), and one that changes `Cargo.lock`, the toolchain or
the root `[profile]` sections adds its own test caches (1.8 GB), so main and two such pull requests stay under
7.5 GB, or 10 GB with main's Zig cache whole. Each ccache holds only the objects its last run used, and at most
1 GB (`CCACHE_MAXSIZE`). In CI the templates' Zig cache folders link to the native one, which saves their libc++
and MinGW builds once. Clippy, the Linux installer and the release builds keep no Rust cache.

Main's native job also deletes, on every branch, the caches under the keys only the workflow before 1.10.0
wrote: `v0-rust-workspace-debug-`, `v0-rust-installer-release-` and `v0-rust-release-` (1.75 GB on main in
November 2026).

## First run of the bindings workflow

`bindings.yml` runs on `pull_request_target`, which uses main's copy of the workflow, so it first runs once it
is on main. Check it once with four pull requests from a fork, then delete the test records.

1. Settings: General → Allow auto-merge; a ruleset on `main` that requires a pull request and the CI checks
   with no bypass; the label `bindings: auto-merge`. Actions → General → the approval rule for fork pull
   request workflows decides whether a maintainer must approve `ci.yml` on a new contributor's pull request
   before its checks run; `bindings.yml` runs either way.
2. Pull request A, one valid record. On a machine with the game, `binding.py new` then `binding.py verify
   --exe … --by <fork owner>` for an address the knowledge base lacks; push only that file. Expected: `validate`
   passes with "Binding check: auto" in its summary, `automerge` adds the label and enables auto-merge, and
   `withdraw` is skipped. The squash merge lands after the CI checks pass.
3. Pull request B, one record with a duplicate key or a pattern that starts with `??`. Expected: `validate`
   fails and lists the error, and `automerge` is skipped.
4. Pull request C, a valid record plus any non-record file. Expected: "Binding check: review", no label.
5. Pinning: in a fourth valid pull request, push a second commit while the CI checks still run. `validate`
   runs again and auto-merge moves to the new head. After the merge, `gh pr view <number> --json
   headRefOid,mergeCommit` and `git diff <headRefOid> <mergeCommit> -- re/bindings` show that the merged
   records are the checked ones.
6. On each merge commit, check whether `ci.yml` and the `main` job of `bindings.yml` ran. Events that
   `GITHUB_TOKEN` causes start no workflows, and an auto-merge it enabled can count as one. Without those
   runs, a tag on that commit builds its binaries in `release.yml`, and `main` checks it on the next push.
7. A maintainer's pull request deletes the test records.
