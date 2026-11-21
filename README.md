# ksmap

An advanced map renderer for Knytt Stories levels.

## Installation

Extract the archive into your Knytt Stories directory, or into a subdirectory such as Third Party Tools. If building from source, extract the `ksmap_data` directory from the archive into the same directory as the executable.

## Usage

### GUI

When launched, ksmap will look for a Knytt Stories installation in the current directory (or above it in the directory tree) and present a list of installed levels.

Alternatively, you can drag and drop a Knytt Stories directory or a level directory onto the executable. In the latter case, the Data directory will be located relative to the level or otherwise relative to the executable.

### CLI

```
ksmap path/to/level
ksmap --help
```

## Features

- Breaks up maps that are too large for one image. By default, this is done by detecting 'islands' of screens that are near one another.
- Fully supports COs and OCOs, including bank 7 recolors.
- Uses in-game graphics rather than editor icons (when applicable).
- Selects a random animation frame and direction for each object (when applicable).
- Synchronizes animations for objects such as lasers, including across screen boundaries.
- Limits objects that can't appear simultaneously, such as the stalky underwater eyeball creatures or red-/green-dotted lasers.
- Simulates transparency for objects such as ghosts.
- Uses correct blend modes (additive, etc.) for objects such as shifts and flames.
- Accurately emulates Multimedia Fusion's software alpha blending (when accurate blending is selected).
- Closely emulates how KS loads World.ini and PNG assets, and countless other obscure KS rendering details.

## Limitations

- KS+ compatibility is prioritized. Most vanilla levels should render correctly, but there may be inaccuracies in niche cases because KS+ features are always on.
- Other mods, such as KS Extended Version, are not supported.
- Anything that relies on collision is unsupported, such as the glitch effect when spiked flyers move through walls or the golden particles emitted by GCs.
- Screen tints are ignored by default. You can enable tints for screens that have one set explicitly with `--tints explicit` (CLI) or in the Drawing Options window (GUI). Many levels rely on implicit tints carried over from other screens. This is very challenging to handle programatically because it requires modeling how the player moves through the level. In some cases, there may even be multiple possible tints for a given screen depending on the path taken to get there.
- Attachments are not supported.
- The frequencies/probabilities of different animations are not modeled, which may sometimes cause unrealistic results. Consequently, certain distracting animations are disabled, including most blinking animations.
