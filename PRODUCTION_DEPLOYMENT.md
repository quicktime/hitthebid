# Production Deployment Guide

## Architecture Overview

For live trading, you need two components:

1. **IB Gateway** - Connects to Interactive Brokers for order execution
2. **Trading Binary** - The `pipeline databento-ib-live` command

Railway is NOT suitable for live trading because:
- IB Gateway requires a persistent connection and stable IP
- Container restarts would disconnect from IB and lose position state

## Recommended Setup: Dedicated VPS

### 1. VPS Requirements

- **Provider**: DigitalOcean, Vultr, Linode, or Hetzner
- **Specs**: 2 CPU, 2GB RAM, 50GB SSD (~$12-24/month)
- **OS**: Ubuntu 22.04 LTS

### 2. Initial Server Setup

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install dependencies
sudo apt install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  openjdk-11-jre \
  xvfb \
  unzip \
  wget \
  supervisor

# Create trading user
sudo useradd -m -s /bin/bash trader
sudo usermod -aG sudo trader
```

### 3. Install IB Gateway with IBC

IBC (IB Controller) allows headless operation of IB Gateway.

```bash
# Switch to trader user
sudo su - trader

# Download IB Gateway (offline installer)
wget https://download2.interactivebrokers.com/installers/ibgateway/stable-standalone/ibgateway-stable-standalone-linux-x64.sh
chmod +x ibgateway-stable-standalone-linux-x64.sh
./ibgateway-stable-standalone-linux-x64.sh -q

# Download IBC
wget https://github.com/IbcAlpha/IBC/releases/download/3.16.0/IBCLinux-3.16.0.zip
unzip IBCLinux-3.16.0.zip -d ~/ibc

# Configure IBC
cat > ~/ibc/config.ini << 'EOF'
LogToConsole=yes
FIX=no
IbLoginId=YOUR_IB_USERNAME
IbPassword=YOUR_IB_PASSWORD
TradingMode=paper
AcceptIncomingConnectionAction=accept
AcceptNonBrokerageAccountWarning=yes
EOF
```

### 4. Deploy Trading Binary

```bash
# Option A: Build on server
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
git clone https://github.com/YOUR_REPO/orderflow-bubbles.git
cd orderflow-bubbles
cargo build --release

# Option B: Copy pre-built binary from local
scp target/release/pipeline trader@your-vps:/home/trader/
```

### 5. Environment Variables

Create `/home/trader/.env`:

```bash
DATABENTO_API_KEY=your_databento_api_key
RUST_LOG=info
```

### 6. Supervisor Configuration

Create `/etc/supervisor/conf.d/trading.conf`:

```ini
[program:ibgateway]
command=/home/trader/ibc/gatewaystart.sh
user=trader
directory=/home/trader
autostart=true
autorestart=true
environment=DISPLAY=":1"

[program:xvfb]
command=/usr/bin/Xvfb :1 -screen 0 1024x768x24
user=trader
autostart=true
autorestart=true

[program:trading]
command=/home/trader/pipeline databento-ib-live --mode paper --contract-symbol NQH6
user=trader
directory=/home/trader
autostart=false
autorestart=unexpected
environment=DATABENTO_API_KEY="%(ENV_DATABENTO_API_KEY)s",RUST_LOG="info"
stdout_logfile=/var/log/trading.log
stderr_logfile=/var/log/trading.error.log
```

### 7. Start Services

```bash
# Start supervisor
sudo supervisorctl reread
sudo supervisorctl update

# Start IB Gateway (wait for it to connect)
sudo supervisorctl start xvfb
sudo supervisorctl start ibgateway

# Verify IB Gateway is running (wait ~30 seconds)
# Then start trading
sudo supervisorctl start trading
```

## Contract Symbol Updates

NQ futures expire quarterly. Update the `--contract-symbol` parameter:

| Quarter | Symbol | Expiry |
|---------|--------|--------|
| Q1 2026 | NQH6   | Mar 2026 |
| Q2 2026 | NQM6   | Jun 2026 |
| Q3 2026 | NQU6   | Sep 2026 |
| Q4 2026 | NQZ6   | Dec 2026 |

Roll to the next contract ~1 week before expiry.

## Monitoring

### View Logs
```bash
# Trading logs
sudo tail -f /var/log/trading.log

# IB Gateway logs
tail -f ~/Jts/*/launcher.log
```

### Check Status
```bash
sudo supervisorctl status
```

## Security Checklist

- [ ] Use SSH keys, disable password auth
- [ ] Enable UFW firewall, allow only SSH
- [ ] Use strong IB password
- [ ] Enable 2FA on IB account
- [ ] Set up monitoring/alerts for disconnections
- [ ] Never commit API keys to git

## Railway (for non-trading services)

Railway can still host:
- The orderflow visualization frontend
- Precompute jobs (scheduled via cron or Railway's scheduled jobs)

Current Railway setup runs demo mode:
```toml
# railway.toml
[deploy]
startCommand = "./orderflow-bubbles --demo"
```
