# Homebrew cask

Brainstorm ships as a cask in the
[fredoliveira/homebrew-tap](https://github.com/fredoliveira/homebrew-tap) tap, next to Recall's and
Ponder's:

```sh
brew install --cask fredoliveira/tap/brainstorm
```

`Casks/brainstorm.rb` in the tap is **generated** by [`generate.sh`](generate.sh) here - edit the
generator, never the tap file directly. It points at the signed, notarized universal
`Brainstorm.dmg` that the Release workflow attaches to each GitHub release.

## Secrets

The Release workflow needs these repository secrets. They are the same ones Recall's and Ponder's
releases use, and they have to be set on this repository too:

| Secret                       | Value                                                             |
| ---------------------------- | ----------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | the Developer ID Application certificate + key (`.p12`), base64   |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password                                               |
| `APPLE_SIGNING_IDENTITY`     | the identity name, `Developer ID Application: <name> (<team id>)` |
| `APPLE_API_ISSUER`           | App Store Connect API issuer ID                                   |
| `APPLE_API_KEY`              | App Store Connect API key ID                                      |
| `APPLE_API_KEY_P8`           | contents of the API key's `.p8` file                              |
| `TAP_PUSH_TOKEN`             | a fine-grained PAT with `contents: write` on the tap repository   |

A tagged release fails rather than ship an unsigned app. Without `TAP_PUSH_TOKEN` the release is
still published; only the cask bump is skipped, and has to be done by hand (below).

To test signing without publishing, run the workflow manually
(`gh workflow run release.yml --ref <branch>`): it builds and notarizes the `.dmg` as an artifact
only.

## On every release: automated

The Release workflow's `tap` job checksums the just-built `Brainstorm.dmg`, regenerates
`Casks/brainstorm.rb` and pushes it to the tap.

## Manual bump (fallback)

```sh
sha="$(gh release download vX.Y.Z -R fredoliveira/brainstorm -p Brainstorm.dmg -O - | shasum -a 256 | cut -d' ' -f1)"
packaging/homebrew/generate.sh X.Y.Z "$sha" > ../homebrew-tap/Casks/brainstorm.rb
```

Commit and push the tap repo.

## Verify

From within the tap repo:

```sh
brew audit --strict --online --cask Casks/brainstorm.rb
brew install --cask Casks/brainstorm.rb
```
