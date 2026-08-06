# Libraries the bundle carries

Windows has no system FFmpeg, so a release has to bring its own. The four
libraries this application actually imports go here, and the bundler copies
them next to the executable:

    avcodec-62.dll  avformat-62.dll  avutil-60.dll  swresample-6.dll

They come from the same package `FFMPEG_DIR` points at while building. See
`docs/BUILD.md` for the command.

The libraries themselves are not in this repository. They are somebody else's
build, they are large, and which one you use is a licensing decision you make
rather than one made for you.
