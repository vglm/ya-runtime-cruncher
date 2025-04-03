#!/bin/bash
git clone https://github.com/vglm/crunch-on-golem.git
(cd crunch-on-golem && git checkout 5a88dbb560e26fa3c547dde40df4d32b29306267 && npm install)
