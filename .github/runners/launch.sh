#!/usr/bin/env bash
# Starts an EC2 spot instance that runs one job of this workflow run, and writes the label that job runs on
# to $GITHUB_OUTPUT as <output>. Any failure writes the fallback hosted runner label instead, so the job still runs.
# Usage: launch.sh <output> <image kind> <instance types, comma separated, preferred first> <fallback label>
set -uo pipefail

NAME=$1 KIND=$2 TYPES=$3 FALLBACK=$4
REGION=${REGION:-ap-south-2}
API=https://api.github.com/repos/$GITHUB_REPOSITORY/actions/runners

fallback() {
  echo "::warning title=EC2 runner::$NAME: $1; the job runs on $FALLBACK"
  echo "$NAME=$FALLBACK" >> "$GITHUB_OUTPUT"
  exit 0
}

TOKEN=$(aws ssm get-parameter --region "$REGION" --name /swofty-ci/github-token --with-decryption \
  --query Parameter.Value --output text) || fallback "the GitHub token could not be read"
[ "${GITHUB_ACTIONS:-}" = true ] && echo "::add-mask::$TOKEN"
IMAGE=$(aws ec2 describe-images --region "$REGION" --owners self --filters "Name=tag:swofty-ci-image,Values=$KIND" Name=state,Values=available \
  --query 'sort_by(Images,&CreationDate)[-1].ImageId' --output text)
[ -n "$IMAGE" ] && [ "$IMAGE" != None ] || fallback "there is no $KIND image"
SG=$(aws ec2 describe-security-groups --region "$REGION" --filters Name=group-name,Values=swofty-ci-runners \
  --query 'SecurityGroups[0].GroupId' --output text)
SUBNETS=$(aws ec2 describe-subnets --region "$REGION" --filters Name=default-for-az,Values=true --query 'Subnets[].SubnetId' --output text)

# The label is the runner's name and its only custom label; nothing else can guess it to take the instance.
LABEL=ec2-$KIND-$(cat /proc/sys/kernel/random/uuid)
if [ "$KIND" = windows ]; then WORK='D:\a'; else WORK=/home/runner/work; fi
JIT=$(curl -fsS -X POST -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' "$API/generate-jitconfig" \
  -d "$(jq -n --arg name "$LABEL" --arg work "$WORK" '{name: $name, runner_group_id: 1, labels: [$name], work_folder: $work}')") ||
  fallback "GitHub did not register the runner"
RUNNER_ID=$(jq -r .runner.id <<< "$JIT")
CONFIG=$(jq -r .encoded_jit_config <<< "$JIT")

if [ "$KIND" = windows ]; then
  USERDATA="<powershell>
Set-Content C:\\actions-runner\\.jitconfig '$CONFIG'
\$action = New-ScheduledTaskAction -Execute powershell.exe -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\\actions-runner\\swofty-run.ps1'
Register-ScheduledTask swofty-runner -Action \$action -User SYSTEM -RunLevel Highest | Out-Null
Start-ScheduledTask swofty-runner
</powershell>"
  DISKS='[{"DeviceName":"/dev/sda1","Ebs":{"VolumeSize":100,"VolumeType":"gp3","Iops":6000,"Throughput":500,"DeleteOnTermination":true}},
    {"DeviceName":"/dev/xvdb","Ebs":{"VolumeSize":150,"VolumeType":"gp3","Iops":6000,"Throughput":500,"DeleteOnTermination":true}}]'
else
  USERDATA="#!/bin/bash
echo '$CONFIG' > /opt/actions-runner/.jitconfig
systemd-run --unit swofty-runner /opt/actions-runner/swofty-run.sh"
  DISKS='[{"DeviceName":"/dev/sda1","Ebs":{"VolumeSize":150,"VolumeType":"gp3","Iops":6000,"Throughput":500,"DeleteOnTermination":true}}]'
fi

TAGS="Tags=[{Key=Name,Value=swofty-ci-$KIND},{Key=swofty-ci,Value=runner},{Key=swofty-ci-run,Value=$GITHUB_RUN_ID},{Key=swofty-ci-runner,Value=$LABEL}]"
for TYPE in ${TYPES//,/ }; do
  for SUBNET in $(shuf -e $SUBNETS); do
    if OUT=$(aws ec2 run-instances --region "$REGION" --image-id "$IMAGE" --instance-type "$TYPE" --subnet-id "$SUBNET" \
        --security-group-ids "$SG" --instance-initiated-shutdown-behavior terminate \
        --instance-market-options 'MarketType=spot,SpotOptions={SpotInstanceType=one-time,InstanceInterruptionBehavior=terminate}' \
        --metadata-options HttpTokens=required,HttpEndpoint=enabled --block-device-mappings "$DISKS" \
        --tag-specifications "ResourceType=instance,$TAGS" "ResourceType=volume,$TAGS" \
        --user-data "$USERDATA" --query 'Instances[0].InstanceId' --output text 2>&1); then
      echo "$NAME: $OUT ($TYPE, $SUBNET) runs $LABEL"
      echo "$NAME=$LABEL" >> "$GITHUB_OUTPUT"
      exit 0
    fi
    echo "$NAME: $TYPE in $SUBNET: $(grep -o 'An error occurred ([A-Za-z.]*)' <<< "$OUT" || echo "$OUT")"
  done
done
curl -fsS -X DELETE -H "Authorization: Bearer $TOKEN" "$API/$RUNNER_ID" || true
fallback "no instance started ($TYPES)"
