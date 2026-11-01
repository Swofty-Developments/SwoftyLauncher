#!/usr/bin/env bash
# Terminates this workflow run's EC2 runners that are still up in any region (a cancelled run leaves instances
# waiting for jobs that never come) and removes their runner registrations.
set -uo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
mapfile -t REGIONS < "$HERE/regions"
API=https://api.github.com/repos/$GITHUB_REPOSITORY/actions/runners

LEFT=$(for REGION in "${REGIONS[@]}"; do
  aws ec2 describe-instances --region "$REGION" \
    --filters "Name=tag:swofty-ci-run,Values=$GITHUB_RUN_ID" Name=instance-state-name,Values=pending,running \
    --query 'Reservations[].Instances[].[InstanceId, Tags[?Key==`swofty-ci-runner`]|[0].Value]' --output text |
    sed "s/^/$REGION /"
done)
[ -n "$LEFT" ] || { echo "no instances left"; exit 0; }

TOKEN=$(aws ssm get-parameter --region "${REGIONS[0]}" --name /swofty-ci/github-token --with-decryption --query Parameter.Value --output text)
[ "${GITHUB_ACTIONS:-}" = true ] && echo "::add-mask::$TOKEN"
while read -r REGION INSTANCE LABEL; do
  aws ec2 terminate-instances --region "$REGION" --instance-ids "$INSTANCE" >/dev/null && echo "terminated $INSTANCE in $REGION ($LABEL)"
  ID=$(curl -fsS -H "Authorization: Bearer $TOKEN" "$API?per_page=100" | jq -r --arg name "$LABEL" '.runners[] | select(.name == $name) | .id')
  [ -n "$ID" ] && curl -fsS -X DELETE -H "Authorization: Bearer $TOKEN" "$API/$ID" && echo "removed runner $LABEL"
done <<< "$LEFT"
exit 0
