#!/bin/env bash
apt update
apt install -y curl net-tools
su -c "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | bash" ubuntu
su -c "\. /home/ubuntu/.nvm/nvm.sh; nvm install 22" ubuntu

# Development additions
passwd -d ubuntu
apt install -y sudo

echo "Use \"docker exec -it -u ubuntu -w /workspace nubila-dev bash\" in order to start the development commands"
echo "Use \"docker exec -it -u ubuntu -w /workspace/cli nubila-dev bash -ic \"/debug.sh\"\" in order to start the CLI in debug mode on port 3000"
sleep infinity