#!/usr/bin/env bash
# Library/tool: no Kellnr publish, no image. make b still lints, tests, and release-builds.
export SKIP_CRATE=true
export SKIP_DOCKER_IMAGE=true
export PUBLISH_CRATE=false
