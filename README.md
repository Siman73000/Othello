<h1 align="center">
  <img src="Othello.png"
       alt="Othello OS icon"
       width="32"
       height="32">
  Othello OS
</h1> <p align="center"> <em>Bare-metal x86_64 playground for bootloaders, kernels, and real-time experiments.</em> </p> <p align="center"> <img alt="arch" src="https://img.shields.io/badge/arch-x86__64-informational?style=for-the-badge"> <img alt="langs" src="https://img.shields.io/badge/languages-Assembly%20%7C%20Rust%20%7C%20C-orange?style=for-the-badge"> <img alt="status" src="https://img.shields.io/badge/status-research%20/%20edu-blueviolet?style=for-the-badge"> <img alt="version" src="https://img.shields.io/badge/version-1.0-success?style=for-the-badge"> </p> <p align="center"> <a href="#overview">Overview</a> · <a href="#boot-pipeline">Boot Pipeline</a> · <a href="#features--design-goals">Features</a> · <a href="#repo-layout">Repo Layout</a> · <a href="#getting-started">Getting Started</a> · <a href="#hacking-points">Hacking Points</a> · <a href="#roadmap--status">Roadmap</a> </p>

# Clone the repository
git clone https://github.com/<your-user>/Othello-OS.git
cd Othello-OS

# Go to the build orchestration directory
cd OS_Build

# Build the disk image and run it (Linux/macOS)
./build-and-run.sh

# Or on Windows (PowerShell)
# ./build-and-run.ps1
