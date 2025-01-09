# glance-widget-bluesky

This is an [extension](https://github.com/glanceapp/glance/blob/main/docs/configuration.md#extension) to [Glance](https://github.com/glanceapp/glance/tree/main), enabling you to grab content from Bluesky and display it on your dashboard.

![Selfhosted](/images/selfhosted.png)

## Usage

### Docker Compose

Uses: [hortonew/glance-widget-bluesky](https://hub.docker.com/repository/docker/hortonew/glance-widget-bluesky/general) from DockerHub.

Put this in your docker-compose.yml

```yaml
services:
  glance:
    image: glanceapp/glance
    # ...

  glance-widget-bluesky:
    image: hortonew/glance-widget-bluesky
    ports:
      - '8081:8080'
    restart: unless-stopped
    env_file:
      - ./.env
```

Make a .env file and add in your Bluesky credentials:

```ini
BLUESKY_BASE_URL=https://bsky.social
BLUESKY_USERNAME=YourUsername
BLUESKY_PASSWORD=YourPassword
```

### Glance Config

Put this in your glance.yml

```yaml
pages:
  - name: Home
    columns:
      - size: full
        widgets:
            - type: extension
                url: http://<your ip or hostname>:<your port> # e.g. http://192.168.1.50:8081
                allow-potentially-dangerous-html: true
                cache: 1s
                parameters:
                    # https://docs.bsky.app/docs/api/app-bsky-feed-search-posts
                    # Required
                    title: "#rustlang security" # should be quoted
                    tags: rustlang,security # each additional tag gets ANDed together

                    # Optional
                    # Content
                    since: -4h # -[int][d|h|m|s]
                    limit: 10
                    collapse-after: 5
                    sort: latest # options: latest, top
                    debug: false # shows what parameters are set

                    # Styling
                    # Note: colors are any valid hex color values, without the #
                    # The colors below match Teal City: https://github.com/glanceapp/glance/blob/main/docs/themes.md#teal-city
                    text-color: 000
                    author-color: 666
                    text-hover-color: 888
                    text-visited-color: 666
                    author-hover-color: AAA
                    hide-stats: false
                    hide-datetime: false
                    hide-author: false
```

## Build from source

```sh
# Test it
cargo run
curl http://localhost:8081/?tags=rustlang&limit=5&sort=top&since=-24h

# Build
docker build -t glance-widget-bluesky:latest .

# Build for specific platform
docker build --platform=linux/amd64 -t glance-widget-bluesky:latest .

# Test it in docker
docker run -dit --rm -p 8081:8080 glance-widget-bluesky:latest
curl http://localhost:8081/?tags=rustlang&limit=5&sort=top&since=-24h
```
