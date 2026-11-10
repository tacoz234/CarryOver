# Carryover website

The product website lives alongside the app so its copy, releases, and brand can stay in sync. It is a static page with local CSS, progressive JavaScript enhancements, and no npm dependencies, external fonts, trackers, or server API.

From the repository root:

```sh
python3 website/build.py
python3 -m http.server 8000 --directory website/dist
```

Open `http://localhost:8000`. The build copies the shared app icon from `assets/` into `website/dist/assets/`. Deploy the contents of `website/dist/` to any static host, including GitHub Pages. All paths are relative, so hosting under a subdirectory works too. Build output is ignored; edit the source files directly.

## GitHub Pages

The public website is published at **https://tacoz234.github.io/CarryOver/**.

[The Website workflow](../.github/workflows/website.yml) builds and deploys the site when `website/`, the shared app icon, or the workflow changes on `clipboard-sync-v0`. You can also run it manually from GitHub Actions. Only `website/dist/` is uploaded; app source, documentation, and hosting metadata are not published. The repository's Pages publishing source is set to **GitHub Actions**.

If the default branch is renamed, update both the workflow's push branch filter and its deployment condition. To use a custom domain later, configure it in the repository's Pages settings.

## Content and behavior

The header menu and text/image example use JavaScript. The page, anchor navigation, download links, and native FAQ disclosures also work without JavaScript. The product composition is labeled as an illustrative preview, with synthetic clipboard content, rather than an app screenshot.

Download buttons use stable GitHub `releases/latest/download/` asset links. When release filenames or platform support change, update the download section and check it against the main README. Documentation links currently target `clipboard-sync-v0`, the repository's existing documentation branch.

`.openai/hosting.json` retains the identity and static output directory for the private Sites preview. It contains no credentials. Hosting elsewhere does not require it.

Sites publication uses a standalone mirror of this directory with the shared icon copied into `assets/`; `build.py` supports that layout too. Carryover's Git repository remains the source of truth.
