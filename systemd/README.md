# Systemd Integration for docker-dns

This directory contains systemd service files for running docker-dns as a system service.

## Installation of docker-dns

### 1. Build and install the binary

First, build the docker-dns binary in release mode:

```bash
cargo build --release
sudo cp target/release/docker-dns /usr/local/bin/
```

### 2. Install the service file

Copy the service file to the systemd directory:

```bash
sudo cp systemd/docker-dns.service /etc/systemd/system/
```

### 3. Reload systemd and enable the service

```bash
sudo systemctl daemon-reload
sudo systemctl enable docker-dns.service
sudo systemctl start docker-dns.service
```

### 4. Testing the DNS Server

Once the service is running, you can test it using `dig` or `nslookup`:

```bash
# Query a Docker container (replace 'mycontainer' with your container name)
dig @localhost -p 5053 mycontainer.docker

# Or using nslookup
nslookup -port=5053 mycontainer.docker localhost 
```

## Configuration of systemd resolved:

### 1. Take note of your resolved configuration

Optional: Run `resolvectl status` to view and save your current configuration (for comparison below).

### 2. Install the `docker-dns.conf` configuration file

```bash
sudo cp systemd/docker-dns.conf /etc/systemd/resolved.conf.d/
```

Note the option to enable `.docker` domain search. See [`docker-dns.conf`](docker-dns.conf) for info.


### 3. Ensure that resolved is configured to use DNS Stub mode

Edit `/etc/systemd/resolved.conf`, veryifying that `DNSStubListener=yes`.

### 4. Restart services

```bash
sudo systemctl restart systemd-networkd
sudo systemctl restart systemd-resolved
sudo systemctl restart docker # Docker might need a restart to update network configuration
```

### 5. Verify correct configuration

Run `resolvectl status` to verify configuration.

* `Global` section should have the following:

   ```bash
   Current DNS Server: 127.0.0.1:5053
          DNS Servers: 127.0.0.1:5053
           DNS Domain: ~docker
   ```

* Your default link section should have the following:
   ```bash
   Link <n> (<iface name>)
       Current Scopes: DNS
            Protocols: +DefaultRoute <other options>
   Current DNS Server: <ip x.x.x.x>
          DNS Servers: <ip x.x.x.x> <ip y.y.y.y> <ip z.z.z.z>
   ```

   where the `<ip x.x.x.x> <ip y.y.y.y> <ip z.z.z.z>` adresses are your original upstream DNS server addresses.

### 6. Test DNS lookup

```bash
# Using dig
dig mycontainer.docker
   
# Or using nslookup
nslookup mycontainer.docker

# Check that upstream still works
dig google.com
nslookup google.com
```
