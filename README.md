<img width="1200" height="450" alt="image" src="https://github.com/user-attachments/assets/9e80fcd3-6ec0-4eca-939c-a5a162ff5860" />

Buslane ingests timetable and fare data from the [Bus Open Data Service](https://data.bus-data.dft.gov.uk/) and serves it as an API, making it possible to use in modern web apps easily. It powers timetable and fare data in [bus.rip](https://bus.rip/) and [Metro](https://metro.transit.fyi/).

<details>
<summary><b>Why not use BODS directly?</b></summary>
    
BODS doesn’t provide a proper API for timetables or fares - instead, it provides individual XML files for every service/fare, which is near impossible to use easily in a modern web app. For timetables, there is a bulk GTFS export instead of having to use XML, but it still requires downloading and processing.
</details>

[API Docs](https://docs.transit.fyi/buslane)

## How it works

It consists of two Rust import jobs, each with their own SQLite database, and a TypeScript API server, all running within Docker. Every day at 2-3am, it fetches the bulk exports from BODS, then applies and processes the differences.

It’s designed to run without crazy resources - in production, it runs on a Raspberry Pi 4 with 4 GB RAM and an NVME SSD connected over USB.

## Get started

Once you’ve cloned the repo (and assuming you have Docker installed):

- **Make a copy of the `.env.example` file, and set it up**<br/>
Mainly, you’ll need to set an auth token for accessing the API, and directories to store data in.<br/>
You should ensure the UID/GID you have set owns the data directories (use `chown` if not).<br/>
`cp .env.example .env`
- **Build the Docker images**<br/>
`docker compose build`
- **Run an initial import**<br/>
Timetables import: `docker compose --profile jobs run --rm --no-deps importer`<br/>
Fares import: `docker compose --profile jobs run --rm --no-deps fares-importer`
- **Start the API server**<br/>
`docker compose up -d --no-build api`
- **Install the import service (runs daily imports)**
```
    sudo install -m 644 deploy/systemd/buslane-import.service deploy/systemd/buslane-import.timer /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now buslane-import.timer
    systemctl list-timers buslane-import.timer
    journalctl -u buslane-import.service
```
