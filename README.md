# glance-widget-bluesky

## Usage

```yaml
  - type: extension
	url: http://192.168.1.50:8080
	allow-potentially-dangerous-html: true
	cache: 1s
	parameters:
		tags: selfhosted # each additional tag gets ANDed together
		limit: 5
		debug: false
		text_color: 000
		author_color: 666
		text_hover_color: 888
		author_hover_color: AAA
        since: -4h # -[int][d|h|m|s]
```
