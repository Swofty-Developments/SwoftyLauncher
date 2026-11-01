import datetime
import os

import boto3

# The swofty-ci-reaper Lambda, every 10 minutes: terminates CI runners older than their 90-minute shutdown
# timer allows and bake instances left past two hours, in every region, whatever happened to the job, the
# instance or GitHub.
LIMITS = {"runner": datetime.timedelta(minutes=100), "bake": datetime.timedelta(hours=2)}


def reap(region, now):
    ec2 = boto3.client("ec2", region_name=region)
    pages = ec2.get_paginator("describe_instances").paginate(Filters=[
        {"Name": "tag:swofty-ci", "Values": list(LIMITS)},
        {"Name": "instance-state-name", "Values": ["pending", "running", "stopping", "stopped"]},
    ])
    old = []
    for page in pages:
        for reservation in page["Reservations"]:
            for instance in reservation["Instances"]:
                kind = next(t["Value"] for t in instance["Tags"] if t["Key"] == "swofty-ci")
                if now - instance["LaunchTime"] > LIMITS[kind]:
                    old.append(instance["InstanceId"])
    if old:
        ec2.terminate_instances(InstanceIds=old)
    return old


def handler(event, context):
    now = datetime.datetime.now(datetime.timezone.utc)
    terminated = {region: reap(region, now) for region in os.environ["REGIONS"].split()}
    print({"terminated": terminated})
    return {"terminated": terminated}
