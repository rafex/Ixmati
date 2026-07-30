# Ixmati v{VERSION}

## Highlights

- {summary}

## Changelog

{changelog}

## Installation

```bash
# Binarios + instalador
curl -sSL https://github.com/rafex/Ixmati/releases/download/v{VERSION}/ixmati-{VERSION}-linux-amd64.tar.gz | tar xz
cd ixmati-{VERSION}-linux-amd64
sudo ./install.sh

# O online (una línea)
curl -sSL https://raw.githubusercontent.com/rafex/Ixmati/main/scripts/install.sh | bash -s -- --version {VERSION}
```

## Container images

```
ghcr.io/rafex/ixmati-api:{VERSION}
ghcr.io/rafex/ixmati-writer:{VERSION}
ghcr.io/rafex/ixmati-projector:{VERSION}
ghcr.io/rafex/ixmati-supervisor:{VERSION}
ghcr.io/rafex/ixmati-reconciler:{VERSION}
ghcr.io/rafex/ixmati-mosquitto:{VERSION}
ghcr.io/rafex/ixmati-litestream:{VERSION}
```

## Quickstart

```bash
cd examples/quickstart
docker compose up -d
./e2e-test.sh
```

## Checksums

```
ixmati-{VERSION}-linux-amd64.tar.gz: {sha256}
```
