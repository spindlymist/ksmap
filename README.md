# ksmap

An advanced map renderer for Knytt Stories levels. If you want to use the source code for something, open an issue or contact me and I'll add a license. (Otherwise, I'll get around to it eventually, surely.)

## Usage

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
- Closely emulates how KS loads World.ini and PNG assets, and countless other obscure KS rendering details.

## Limitations

- KS+ compatibility is prioritized. Most vanilla levels should render correctly, but there may be inaccuracies in niche cases because KS+ features are always on.
- Anything that relies on collision is unsupported, such as the glitch effect when spiked flyers move through walls or the golden particles emitted by GCs.
- Screen tints are ignored by default. You can enable tints for screens that have one set explicitly with `--tints explicit`. Many levels rely on implicit tints carried over from other screens. This is very challenging to handle programatically because it requires modeling how the player moves through the level. In some cases, there may even be multiple possible tints for a given screen depending on the path taken to get there.
- Attachments are not supported.
- The frequencies/probabilities of different animations are not modeled, which may sometimes cause unrealistic results. Consequently, certain distracting animations are disabled, including most blinking animations.
- The alpha blending algorithm differs slightly from the one used by KS, so off-by-one errors are fairly common. These are more or less imperceptible, but could be relevant in rare cases, such as on screens with bitwise tints (AND, OR, and XOR).
