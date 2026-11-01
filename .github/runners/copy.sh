#!/usr/bin/env bash
# Copies the newest image of a kind from the home region (the first line of regions) to the other regions,
# and keeps the two newest of that kind in each: ./copy.sh linux-2604 | linux-2204 | windows
set -euo pipefail

KIND=$1
HERE=$(cd "$(dirname "$0")" && pwd)
mapfile -t REGIONS < "$HERE/regions"
HOME_REGION=${REGIONS[0]}

read -r IMAGE NAME < <(aws ec2 describe-images --region "$HOME_REGION" --owners self \
  --filters "Name=tag:swofty-ci-image,Values=$KIND" Name=state,Values=available \
  --query 'sort_by(Images,&CreationDate)[-1].[ImageId,Name]' --output text)
[ -n "$IMAGE" ] && [ "$IMAGE" != None ] || { echo "no $KIND image in $HOME_REGION" >&2; exit 1; }

declare -A COPIES
for REGION in "${REGIONS[@]:1}"; do
  COPY=$(aws ec2 describe-images --region "$REGION" --owners self --filters "Name=name,Values=$NAME" \
    --query 'Images[0].ImageId' --output text)
  if [ "$COPY" = None ]; then
    COPY=$(aws ec2 copy-image --region "$REGION" --source-region "$HOME_REGION" --source-image-id "$IMAGE" --name "$NAME" \
      --tag-specifications "ResourceType=image,Tags=[{Key=swofty-ci-image,Value=$KIND}]" \
      "ResourceType=snapshot,Tags=[{Key=swofty-ci-image,Value=$KIND}]" --query ImageId --output text)
    echo "$REGION: copying $NAME as $COPY"
  fi
  COPIES[$REGION]=$COPY
done

for REGION in "${REGIONS[@]:1}"; do
  for _ in $(seq 1 120); do
    STATE=$(aws ec2 describe-images --region "$REGION" --image-ids "${COPIES[$REGION]}" --query 'Images[0].State' --output text)
    [ "$STATE" = available ] && break
    sleep 30
  done
  [ "$STATE" = available ] || { echo "$REGION: ${COPIES[$REGION]} is $STATE after an hour" >&2; exit 1; }
  for OLD in $(aws ec2 describe-images --region "$REGION" --owners self --filters "Name=tag:swofty-ci-image,Values=$KIND" \
      --query 'sort_by(Images,&CreationDate)[:-2].ImageId' --output text); do
    SNAPSHOTS=$(aws ec2 describe-images --region "$REGION" --image-ids "$OLD" --query 'Images[0].BlockDeviceMappings[].Ebs.SnapshotId' --output text)
    aws ec2 deregister-image --region "$REGION" --image-id "$OLD" >/dev/null
    for SNAPSHOT in $SNAPSHOTS; do aws ec2 delete-snapshot --region "$REGION" --snapshot-id "$SNAPSHOT"; done
    echo "$REGION: removed $OLD"
  done
  echo "$REGION: ${COPIES[$REGION]} ready"
done
