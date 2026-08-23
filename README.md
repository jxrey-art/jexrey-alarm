# JEXREY ALARM CONTROL SYSTEM

Console de contrôle d'alarmes multiples pour Windows, écrite en **Rust**
(`egui`/`eframe`). Compilation en un seul `.exe` autonome, sans runtime à
installer sur la machine cible.

---

## A. Pourquoi Rust (et pas Python/C#/Electron)

| Priorité demandée                        | Ce que Rust apporte |
|-------------------------------------------|----------------------|
| Zéro dépendance à installer manuellement  | Le binaire compilé est **statiquement lié** : ni .NET, ni JVM, ni Node.js, ni Python n'est nécessaire sur la machine qui exécute `JEXREYAlarm.exe`. |
| Performance / stabilité                   | Code natif compilé, pas de garbage collector, pas de pauses aléatoires sur une appli censée tourner des jours. |
| Faible RAM/CPU                            | Une appli `egui` de ce type consomme typiquement 30–60 Mo de RAM contre plusieurs centaines de Mo pour un équivalent Electron. |
| `.exe` facile à produire                  | Une seule commande : `cargo build --release`. |
| Audio fiable (MP3/WAV/OGG)                | `rodio` + `symphonia` décodent nativement ces trois formats, sans codec externe à installer. |

Les crates listées dans `Cargo.toml` (`egui`, `rodio`, `sysinfo`, `rfd`,
`serde`...) sont des **dépendances de compilation** : `cargo` les télécharge
et les compile *à l'intérieur* du binaire final. Ce ne sont pas des
installations que l'utilisateur final doit faire — c'est la différence
avec "il faut installer Python + pip + pygame" que vous vouliez éviter.

---

## B. Structure des fichiers

```
jexrey-alarm/
├── Cargo.toml          # dépendances + réglages de compilation release
├── Cargo.lock           # versions exactes testées (à ne pas supprimer)
└── src/
    ├── main.rs           # point d'entrée, création de la fenêtre
    ├── app.rs            # état de l'appli + toute la mise en page UI
    ├── alarm.rs           # modèle de données + machine à états d'une alarme
    ├── scheduler.rs        # vérification du déclenchement, anti-doublon
    ├── audio.rs             # moteur audio, boucle infinie, bip de secours
    ├── config.rs              # sauvegarde/chargement de alarms.json
    ├── system_info.rs          # thread de télémétrie (CPU/RAM réels)
    ├── event_log.rs              # journal d'événements borné en mémoire
    ├── radar.rs                    # visualisation "radar temporel"
    ├── theme.rs                      # palette de couleurs + style global
    └── widgets.rs                      # composants réutilisables (jauges, badges...)
```

---

## C. Installation (poste de développement)

1. Installer Rust via **[rustup.rs](https://rustup.rs)** (choisir l'installation
   par défaut). Cela installe `cargo` et `rustc` à jour — utilisez bien
   rustup, pas un paquet Rust ancien fourni par un gestionnaire de paquets tiers.
2. Vérifier :
   ```
   cargo --version
   rustc --version
   ```
3. Ouvrir un terminal (PowerShell ou cmd) dans le dossier `jexrey-alarm/`.

Aucune autre dépendance à installer : pas de Python, pas de Node.js, pas
de SDK supplémentaire. `rfd` (sélecteur de fichier) utilise directement
la boîte de dialogue native Windows ; `rodio` utilise directement
l'API audio Windows (WASAPI) via `cpal`.

---

## D. Compilation → `.exe`

### Lancer en développement (avec console de debug)
```
cargo run
```

### Compiler la version finale optimisée
```
cargo build --release
```

Le premier build prend **quelques minutes** (optimisation LTO complète,
volontaire pour un exécutable plus petit et plus rapide — voir
`[profile.release]` dans `Cargo.toml`). Les builds suivants, si vous ne
modifiez que du code, sont bien plus rapides.

Le résultat est ici :
```
target\release\JEXREYAlarm.exe
```

C'est un exécutable Windows **autonome** — copiez-le où vous voulez,
double-cliquez, aucune installation de dépendance nécessaire. Aucun
terminal ne s'ouvre en mode release (`windows_subsystem = "windows"` dans
`main.rs`).

---

## E. Mode autonome (exécuter sur un autre PC Windows sans rien installer)

`cargo build --release` produit déjà un binaire statiquement lié — sur un
Windows 10/11 x86_64 propre, il suffit de copier `JEXREYAlarm.exe` et de
le lancer. Rien d'autre n'est requis (pas de redistribuable Visual C++ à
installer séparément : les dépendances C nécessaires sont liées
statiquement par le linker MSVC/MinGW par défaut de Rust sur cible
Windows).

Si vous voulez distribuer un dossier "portable" propre :
```
mkdir dist
copy target\release\JEXREYAlarm.exe dist\
```
C'est tout — `dist\JEXREYAlarm.exe` est votre livrable final.

---

## F. Explication de l'architecture

### Vue d'ensemble
L'appli est **mono-thread pour toute la logique d'alarme** (pas de
`Arc<Mutex<...>>`, pas de risque de course entre l'UI qui modifie une
alarme et un thread qui la déclenche au même instant). `eframe` garantit
que `update()` est rappelé régulièrement tant que l'appli demande un
repaint (`ctx.request_repaint_after(...)`), donc le "scheduler" est en
réalité une simple fonction appelée à chaque frame, qui se limite
elle-même à un vrai check par seconde (`scheduler.rs`).

Un seul vrai thread d'arrière-plan existe : la **télémétrie système**
(`system_info.rs`), parce que mesurer le CPU % avec `sysinfo` nécessite
deux lectures espacées de 200 ms — bloquer l'UI pendant ce temps aurait
été visible. Ce thread publie juste sa dernière mesure dans un petit
`Arc<Mutex<SystemSnapshot>>` que l'UI lit sans jamais attendre dessus.

### Gestion des alarmes (`alarm.rs`)
Chaque alarme a :
- un **switch utilisateur** `enabled` (ON/OFF, persistant) ;
- un **état de cycle de vie** `state` : `Scheduled` → `Ringing` →
  `Completed` ;
- `last_triggered_on`, la date du dernier déclenchement — c'est ce
  champ, et lui seul, qui empêche une alarme de sonner deux fois dans la
  même minute, **et** qui permet à une alarme répétitive de redevenir
  automatiquement "active" le jour suivant sans job de minuit séparé
  (`display_state()` recalcule ça à la volée).

### Scheduler (`scheduler.rs`)
`Scheduler::tick()` compare l'horodatage courant (tronqué à la seconde)
à celui du dernier passage : s'ils sont identiques, il ne refait rien
(protection contre les appels multiples par seconde dus au taux de
rafraîchissement). Sinon, il parcourt les alarmes, déclenche celles dont
`should_trigger()` répond `true`, démarre l'audio, et retourne la liste
des événements pour le journal.

### Audio en boucle infinie (`audio.rs`)
Exigence absolue du cahier des charges. `rodio::Decoder` ne se clone pas
facilement, donc au lieu de rouvrir le fichier à chaque tour de boucle
(lent, et un point de défaillance de plus si le fichier devient
inaccessible), le fichier est **décodé une seule fois en mémoire** dans
un `SamplesBuffer` clonable, puis bouclé avec `.repeat_infinite()`. Ce
buffer, une fois en RAM, ne peut plus "caler" en attendant un disque.
Chaque alarme a son propre `Sink` (`HashMap<u64, Sink>`), donc plusieurs
alarmes peuvent sonner en même temps sans se marcher dessus. Si le
fichier son est manquant, corrompu, ou absent, un **bip synthétisé** en
secours prend automatiquement le relais — une alarme ne peut jamais
rester silencieuse.

### Sauvegarde (`config.rs`)
`alarms.json` est stocké dans `%APPDATA%\JexreyAlarmSystem\` (pas à côté
de l'exe, qui peut être dans un dossier en lecture seule comme *Program
Files*). L'écriture se fait sur un fichier temporaire puis un
renommage atomique, pour qu'un crash en plein milieu d'une sauvegarde ne
puisse jamais laisser un `alarms.json` à moitié écrit. Si le fichier est
corrompu au chargement, il est sauvegardé en `.corrupt` et l'appli
redémarre avec une configuration vide plutôt que de planter.

### Interface (`app.rs`, `theme.rs`, `widgets.rs`, `radar.rs`)
- Barre du haut : horloge, prochaine alarme, compteur, statut.
- Panneau central : grille des alarmes + journal d'événements — remplacé
  entièrement par le **mode alarme** (bordure pulsante, bouton STOP
  géant) dès qu'une alarme sonne.
- Colonne de droite : radar temporel (angle = heure de l'alarme sur un
  cadran 24h, distance au centre = urgence), système local, télémétrie,
  opérations actives.
- Pendant qu'une alarme sonne, la fenêtre passe automatiquement
  "toujours au premier plan" (`ViewportCommand::WindowLevel`) pour que le
  bouton STOP reste joignable même si l'appli était en arrière-plan.
- Raccourcis clavier : `N` (nouvelle alarme), `ESC` / `ESPACE` (stop).

---

## Tests de robustesse couverts

- Fichier son supprimé/inaccessible/corrompu → bip de secours, jamais de
  silence, jamais de crash.
- Alarme sans son → bip de secours.
- Suppression/modification d'une alarme active → gérée sans plantage.
- Plusieurs alarmes proches ou simultanées → chacune a son propre `Sink`
  audio et son propre bouton STOP dans l'écran d'alarme.
- Fermeture pendant une sonnerie → `on_exit()` sauvegarde quand même
  l'état actuel.
- `alarms.json` corrompu → sauvegardé en `.corrupt`, redémarrage propre
  avec configuration vide.
- Redimensionnement de fenêtre → mise en page responsive (panneaux
  redimensionnables, grille avec `ScrollArea`).
- Journal d'événements → borné à 300 lignes (`event_log.rs`), donc pas
  de croissance mémoire sur une session de plusieurs jours.
