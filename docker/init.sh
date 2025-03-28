#!/bin/env bash
apt update
apt install -y curl python3 python-is-python3 build-essential libx11-dev libxkbfile-dev libsecret-1-dev python3-setuptools
su -c "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | bash" ubuntu
su -c "\. /home/ubuntu/.nvm/nvm.sh; nvm install 22; npm install -g yarn yo generator-theia-extension node-gyp" ubuntu

# Development additions
passwd -d ubuntu
apt install -y sudo

echo "Use \"docker exec -it -u ubuntu -w /workspace nubila-dev bash\" in order to start the development commands"
sleep infinity