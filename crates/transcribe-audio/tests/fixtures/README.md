These files contain a synthetic 440 Hz sine tone, generated for decoder tests.
There are no recordings of people. Each source is 0.1735 seconds at 44.1 kHz;
codec padding can change decoded duration slightly.

Generated with FFmpeg using `-f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.1735 -ac 2 -map_metadata -1` and:

- MP3: `-c:a libmp3lame -b:a 64k`
- FLAC: `-c:a flac`
- Ogg/Vorbis: `-c:a vorbis -strict experimental`
- M4A/AAC: `-c:a aac -b:a 64k`

Tests use Symphonia directly; they do not invoke or require FFmpeg.
