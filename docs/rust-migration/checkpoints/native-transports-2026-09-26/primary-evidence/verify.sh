#!/bin/bash
set -eu
task_root=/root/llmbase-rust-migration-20260920
run_root=$task_root/b9-primary
python_bin=$task_root/b5-registration-fix/candidate/bin/python
cd "$run_root"
sha256sum -c harness.sha256 > source-check.log 2>&1
printf '%s\n' 0 > source-check.exit
cp "$task_root/b8-web-read-config/llmbase-cli-bin" llmbase-cli-bin
printf '%s\n' 'f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8  llmbase-cli-bin' | sha256sum -c - > binary-check.log
step() {
  name=$1
  shift
  set +e
  nice -n10 "$@" > "$name.log" 2>&1
  result=$?
  set -e
  printf '%s\n' "$result" > "$name.exit"
  return "$result"
}
step standalone "$python_bin" verify-standalone.py --binary "$run_root/llmbase-cli-bin" --run-root "$run_root/accepted" --chroot
step reject-expired-gate "$python_bin" reject-expired-gate.py
sha256sum -c harness.sha256 > post-source-check.log 2>&1
printf '%s\n' 0 > post-source-check.exit
printf '%s\n' done > done
