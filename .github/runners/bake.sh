#!/usr/bin/env bash
# Bakes one runner image: ./bake.sh linux-2604 | linux-2204 | windows
# It bakes in the home region (the first line of regions), keeps the two newest of the kind there and copies
# the new one to the other regions (copy.sh); launch.sh picks the newest image tagged swofty-ci-image=<kind>.
set -euo pipefail

KIND=$1
HERE=$(cd "$(dirname "$0")" && pwd)
REGION=$(head -1 "$HERE/regions")
SG=$(aws ec2 describe-security-groups --region "$REGION" --filters Name=group-name,Values=swofty-ci-runners --query 'SecurityGroups[0].GroupId' --output text)

case $KIND in
  linux-2604|linux-2204)
    VERSION=${KIND#linux-}; VERSION=${VERSION:0:2}.${VERSION:2}
    BASE=$(aws ssm get-parameter --region "$REGION" --name "/aws/service/canonical/ubuntu/server/$VERSION/stable/current/amd64/hvm/ebs-gp3/ami-id" --query Parameter.Value --output text 2>/dev/null ||
      aws ssm get-parameter --region "$REGION" --name "/aws/service/canonical/ubuntu/server/$VERSION/stable/current/amd64/hvm/ebs-gp2/ami-id" --query Parameter.Value --output text)
    WINE=$([ "$KIND" = linux-2604 ] && echo 1 || echo 0)
    USERDATA=$(sed "s/@WINE@/$WINE/" "$HERE/bake-linux.sh")
    DISK=30 DEVICE=/dev/sda1 ;;
  windows)
    BASE=$(aws ec2 describe-images --region "$REGION" --owners amazon --filters 'Name=name,Values=Windows_Server-2025-English-Full-Base-*' \
      --query 'sort_by(Images,&CreationDate)[-1].ImageId' --output text)
    USERDATA=$(cat "$HERE/bake-windows.ps1")
    DISK=80 DEVICE=/dev/sda1 ;;
  *) echo "unknown kind $KIND" >&2; exit 2 ;;
esac

echo "baking $KIND from $BASE"
INSTANCE=$(aws ec2 run-instances --region "$REGION" --image-id "$BASE" --instance-type m8a.2xlarge \
  --security-group-ids "$SG" --iam-instance-profile Name=swofty-ci-bake \
  --instance-initiated-shutdown-behavior stop \
  --block-device-mappings "DeviceName=$DEVICE,Ebs={VolumeSize=$DISK,VolumeType=gp3,DeleteOnTermination=true}" \
  --metadata-options HttpTokens=required,HttpEndpoint=enabled \
  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=swofty-ci-bake-$KIND},{Key=swofty-ci,Value=bake}]" \
  --user-data "$USERDATA" --query 'Instances[0].InstanceId' --output text)
echo "instance $INSTANCE"
trap 'echo "bake instance $INSTANCE left running for inspection" >&2' ERR

for _ in $(seq 1 120); do
  STATE=$(aws ec2 describe-instances --region "$REGION" --instance-ids "$INSTANCE" --query 'Reservations[0].Instances[0].State.Name' --output text)
  [ "$STATE" = stopped ] && break
  sleep 30
done
[ "$STATE" = stopped ] || { echo "bake did not finish in an hour" >&2; exit 1; }

NAME="swofty-ci-$KIND-$(date -u +%Y%m%d-%H%M)"
IMAGE=$(aws ec2 create-image --region "$REGION" --instance-id "$INSTANCE" --name "$NAME" \
  --tag-specifications "ResourceType=image,Tags=[{Key=swofty-ci-image,Value=$KIND}]" "ResourceType=snapshot,Tags=[{Key=swofty-ci-image,Value=$KIND}]" \
  --query ImageId --output text)
echo "image $IMAGE ($NAME)"
for _ in $(seq 1 120); do
  IMAGE_STATE=$(aws ec2 describe-images --region "$REGION" --image-ids "$IMAGE" --query 'Images[0].State' --output text)
  [ "$IMAGE_STATE" = available ] && break
  sleep 30
done
[ "$IMAGE_STATE" = available ] || { echo "image $IMAGE is $IMAGE_STATE after an hour" >&2; exit 1; }
trap - ERR
aws ec2 terminate-instances --region "$REGION" --instance-ids "$INSTANCE" >/dev/null

for OLD in $(aws ec2 describe-images --region "$REGION" --owners self --filters "Name=tag:swofty-ci-image,Values=$KIND" \
    --query 'sort_by(Images,&CreationDate)[:-2].ImageId' --output text); do
  SNAPSHOTS=$(aws ec2 describe-images --region "$REGION" --image-ids "$OLD" --query 'Images[0].BlockDeviceMappings[].Ebs.SnapshotId' --output text)
  aws ec2 deregister-image --region "$REGION" --image-id "$OLD"
  for SNAPSHOT in $SNAPSHOTS; do aws ec2 delete-snapshot --region "$REGION" --snapshot-id "$SNAPSHOT"; done
  echo "removed $OLD"
done
echo "done: $IMAGE"
"$HERE/copy.sh" "$KIND"
