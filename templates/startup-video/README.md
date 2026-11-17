# Swofty Intro (startup video)

Replaces F1 25's EA SPORTS startup video with a 6 second Swofty logo intro: the launcher's logo with a soft glow and a light sweep, the SWOFTY wordmark, soft chimes and a spoken "Swofty". The EA intro subtitle line reads "SWOFTY".

It is the finished addon of the guide [Make a startup video](../../docs/make/startup-video.md).

```text
startup-video/
  swofty.json                       one file operation and one assets file
  files/videos/cm_f1_sting.bk2      the video: Bink 2, 1920x1080, 30 fps, 180 frames
  assets/subtitle.json              lang.set for lng_subtitle_ea_sports_intro
  tools/make_intro.py               renders the video (Python 3, numpy, Pillow, ffmpeg)
  tools/voice.ps1                   the voice line, offline Windows TTS
```

Try it:

```sh
swofty dev check templates/startup-video
swofty dev link templates/startup-video
swofty run --apply
```

Disabling the addon, or `swofty restore`, puts the stock video back. It replaces the same file as any other startup video addon, so use one at a time.

MIT. The Swofty logo is the launcher's own mark (`apps/launcher/assets/icon.svg`).
