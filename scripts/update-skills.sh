#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

# 1. Create the destination directory
mkdir -p .agents/skills/rust-skills

# 2. Download the repository archive and extract only SKILL.md and the rules folder
curl -sL https://github.com/leonardomso/rust-skills/archive/refs/heads/master.tar.gz |
    tar -xz -C .agents/skills/rust-skills --strip-components=1 rust-skills-master/SKILL.md rust-skills-master/rules
