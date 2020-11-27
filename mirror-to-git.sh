#!/usr/bin/env bash
# requirements: darcs and git

git init ../icy_matrix-git
touch ../icy_matrix-git/git.marks
darcs convert export --read-marks darcs.marks --write-marks darcs.marks | (cd ../icy_matrix-git && git fast-import --import-marks=git.marks --export-marks=git.marks)
git --git-dir=../icy_matrix-git/.git --work-tree=../icy_matrix-git restore --staged .
git --git-dir=../icy_matrix-git/.git --work-tree=../icy_matrix-git restore .
