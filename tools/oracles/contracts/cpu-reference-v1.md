# CPU Reference v1

The independent CPU reference evaluates the fixture's published rational time
map, Rec.709 metadata, marker schedule, and decoded semantic frame/audio
digests. It must not call editor-core, the editor renderer, or the app's media
pipeline. Its inputs are the checked-in fixture recipe and independently
decoded pixels/samples.
