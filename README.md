# docker-dns

A DNS server that resolves Docker container names to their IP addresses.

## Usage

```txt
$ docker-dns --help
DNS server that resolves Docker container names to their IP addresses. Source: https://github.com/jvcdk/docker-dns

Usage: docker-dns [OPTIONS]

Options:
  -b, --bind <BIND>
          DNS server bind address [default: 0.0.0.0:53]
  -s, --socket <SOCKET>
          Docker socket path [default: /var/run/docker.sock]
      --hit-timeout <HIT_TIMEOUT>
          Cache hit timeout in seconds (how long to cache successful lookups) [default: 60]
      --miss-timeout <MISS_TIMEOUT>
          Cache miss timeout in seconds (how long to wait before retrying failed lookups) [default: 5]
      --docker-timeout <DOCKER_TIMEOUT>
          Docker API communication timeout in seconds [default: 5]
      --suffix <SUFFIX>
          DNS suffix to filter queries (e.g., "docker" or ".docker"). Only queries ending with this suffix will be resolved. The suffix will be stripped before looking up container names [default: ]
  -h, --help
          Print help
  -V, --version
          Print version
```

### Example

This is an example of docker-dns in action. This example is configured as follows:
* `docker-dns` is running with `--suffix docker`: Only handle `.docker` domains.
* [systemd-resolved](https://www.freedesktop.org/software/systemd/man/latest/systemd-resolved.service.html) is configured to delegate `.docker` domains to `docker-dns`.
  * See [systemd/README.md](systemd/README.md) for info.

```bash
# List running containers
$ docker ps
CONTAINER ID  IMAGE  COMMAND                 CREATED             STATUS             PORTS   NAMES
8d85bdeb0125  nginx  "/docker-entrypoint.…"  About a minute ago  Up About a minute  80/tcp  modest_gould
bd618193e4b5  nginx  "/docker-entrypoint.…"  About a minute ago  Up About a minute  80/tcp  zen_montalcini


# Manually find their IP addresses
$ docker inspect modest_gould | jq '.[0].NetworkSettings.Networks.bridge.IPAddress'
"10.200.0.4"

$ docker inspect zen_montalcini | jq '.[0].NetworkSettings.Networks.bridge.IPAddress'
"10.200.0.3"


# Use nslookup to find IP of modest_gould
$ nslookup modest_gould.docker
Server:		127.0.0.53
Address:	127.0.0.53#53

Non-authoritative answer:
Name:	modest_gould.docker
Address: 10.200.0.4

# Use nslookup to find IP of zen_montalcini
$ nslookup zen_montalcini.docker
Server:		127.0.0.53
Address:	127.0.0.53#53

Non-authoritative answer:
Name:	zen_montalcini.docker
Address: 10.200.0.3


# Testing connectivity to service
$ curl zen_montalcini.docker:8080
Hello from container world...
```

## Build

```bash
cargo build --release
```

Executable will be located at `target/release/docker-dns`.

## Systemd Integration

For systemd service configuration and setup instructions, see [systemd/README.md](systemd/README.md).

## License

Licenced under a [BSD 3-Clause Licence](LICENCE).
