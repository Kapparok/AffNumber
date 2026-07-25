# AffNumber

([Hachimi](https://github.com/kairusds/Hachimi-Edge)) plugin based on Heaven functionality that shows **exact Legacy Select affinity numbers**.

Works on **Global and Japanese** version of the game.

On the career **Legacy Select** screen it shows:

- **Total** pair affinity
- **Parent 1** branch
- **Parent 2** branch

Values should be accurate to the in-game ones.

## Install

1. Put `affnumber.dll` in the game folder.

2. In Hachimi's `config.json`:

   ```json
   {
      "load_libraries": [
        "affnumber.dll"
      ]
   }
   ```

3. Launch the game.

## Usage

1. Start a career.
2. Press **P** (default) to show/hide the overlay.

Settings are saved to `<game>/hachimi/affnumber.json`. Change `toggle_key` there (e.g. `"F10"`, `"H"`, `"Insert"`, or `"None"` to disable the hotkey).

## Compatibility

Incompatible with **Heaven** since it shares the same hook.

## Requirements

- ([Hachimi]https://github.com/kairusds/Hachimi-Edge)
- The game (you know which one)

## Credits 

Thanks to ([Night DC (nighty333)](https://github.com/Nighty3333/)) for letting me use his affinity overlay from **Heaven Internal Public** as a base for this plugin.

## License

[MIT License](LICENSE)
