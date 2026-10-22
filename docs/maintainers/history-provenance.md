# Pre-publication source history

Before the first public push on 22 October 2026, the repository owner requested that the seven
local commits use **Fathi-Farhan <fthfarhan12@gmail.com>** for both author and committer.
Their subjects were normalized to Conventional Commits. Original author/committer dates,
message bodies, parent ordering and every source tree were preserved. Commit IDs changed
because Git includes metadata and parent IDs in each commit object.

Historical benchmark JSON and dated reports retain the commit IDs observed during their runs.
They were not rewritten to resemble measurements of a newer build. Use the mapping below to
locate the identical source tree in the public history; binary/plugin hashes and dirty-build
qualifiers in those reports remain necessary.

| Recorded commit before publication         | Public commit with the identical source tree |
| ------------------------------------------ | -------------------------------------------- |
| `b14c99503f3ca2c18f24c074bbfbe48679d2b5ac` | `2df08beddf0261649d0b77a5a60be02d780f2fa4`   |
| `e9e23b0ed210268d06e6697aad8b0356c32e530a` | `f27fa7d8049e57c0d98731948da068395bcb0f8a`   |
| `152adb27c7ff33fdc01a4d5bf8b90991e9157a54` | `a109c46f805d4428311ef0cfcfc8c6532a8665e2`   |
| `51165682e1d3ce8c99054d9e939bd4044dae2b36` | `8767e4ead3848ea7a1e91f2385ee057df523b660`   |
| `fc345d0146eb6a617179e9a3fe6e3d590e6dc00b` | `35b93d6d02c55243ea7845252814d28b0db14ecb`   |
| `14d57f11a854e0babf166f1cd3a0e7ee64d35b97` | `8f9c49dba667251c4a94f31d3162b28cc1b5af53`   |
| `c598d8f21763e0f796cb91477e93e94473c8bdc1` | `eb2eaf8b0311c94645bead88ab6d20c4775f10a9`   |

A full Git bundle of the original history was verified and retained privately before rewriting.
The rewrite checked every replacement tree ID against its original. This changes commit
attribution and subject formatting, not measured source contents or performance results.
