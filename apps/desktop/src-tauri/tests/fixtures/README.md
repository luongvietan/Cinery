# Video QA test fixture

`video_qa_wpt_h264.mp4.b64` is a base64-encoded copy of Web Platform Tests'
`media-source/mp4/test-v-128k-640x480-30fps-10kfr.mp4` fixture.

- Source: <https://github.com/web-platform-tests/wpt/blob/master/media-source/mp4/test-v-128k-640x480-30fps-10kfr.mp4>
- Upstream media type: `video/mp4;codecs="avc1.4D4001"`
- Decoded byte length: `27,764`
- Decoded SHA-256: `1743855560ef42b195a58901fc634881ad1dd6b01394ce8feedd23cfb25a3fbf`
- Upstream license: BSD 3-Clause (`web-platform-tests/wpt`)

The integration test decodes this text fixture at runtime, verifies the
expected H.264/AVC MP4 structure markers, and writes it into a temporary
directory. No system media binary participates in fixture creation or test
execution.
