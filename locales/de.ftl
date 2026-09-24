# Terminaal – deutsche Texte.
#
# Fluent-Syntax: https://projectfluent.org/fluent/guide/
# Jede Meldung hier muss es auch in en.ftl geben, mit denselben Argumenten
# (prüft `cargo test`). Terminalfarben sowie Leerzeichen oder
# Zeilenumbrüche um eine ganze Meldung ergänzt der Code.

## Allgemein

common-save = Speichern
common-cancel = Abbrechen
common-delete = Löschen
common-edit = Bearbeiten
common-remove = Entfernen
common-no = Nein
common-reload = Erneut laden
common-default = Standard
common-name-missing = Der Name fehlt.
common-saved = „{ $name }“ gespeichert.
common-deleted = „{ $name }“ gelöscht.
common-home-unset = $HOME ist nicht gesetzt.
common-file-unreadable = { $path } konnte nicht gelesen werden: { $err }
common-file-invalid = { $path } ist fehlerhaft: { $err }

## Kommandozeile

cli-usage = Aufruf: terminaal [--connect [BENUTZER@]HOST | --quake]
cli-quake-failed = Das Dropdown-Terminal ließ sich nicht starten oder umschalten: { $err }
cli-unknown-argument = Unbekanntes Argument „{ $arg }“.
cli-unexpected-argument = Unerwartetes Argument „{ $arg }“.
cli-connect-needs-host = --connect braucht einen Hostnamen.
cli-unknown-host = Kein gespeicherter oder in ~/.ssh/config eingetragener Host namens „{ $host }“.

## Config-Datei

config-invalid-toml = { $path } ist kein gültiges TOML: { $err }

## Aktionen, die die App ausführt

app-shell-start-failed = { $shell } konnte nicht gestartet werden: { $err }
app-ssh-tab-failed = SSH-Tab für { $target } konnte nicht geöffnet werden: { $err }
app-default-shell-set = { $shell } ist jetzt die Standard-Shell für neue Tabs.
app-language-changed = Sprache umgestellt.

## Kontextmenü (Rechtsklick ins Terminal)

menu-copy = Kopieren
menu-copy-output = Ausgabe kopieren
menu-paste = Einfügen
menu-paste-run = Einfügen und ausführen
menu-broadcast-on = Broadcast für dieses Terminal
menu-broadcast-off = Broadcast beenden
menu-silence-on = Auf Stille achten
menu-silence-off = Nicht mehr auf Stille achten
menu-split-right = Rechts teilen
menu-split-down = Unten teilen
menu-close-pane = Bereich schließen
menu-files = Dateien (SFTP)

## Suche im Scrollback (render/search_bar.rs)

search-prompt = Suchen:
search-no-match = Keine Treffer
search-hint-typing = Enter ↑ · Umschalt+Enter ↓ · Esc
search-hint-jumping = n ↑ · N ↓ · / ändern · Esc

## Shell-Integration (terminal/integration.rs, app.rs)

notify-finished = Befehl fertig
notify-failed = Befehl fehlgeschlagen (Exit-Code { $code })
notify-body = { $tab } – nach { $duration }
notify-silent = Terminal ist still
notify-silent-body = { $tab } – seit { $duration } keine Ausgabe
duration-seconds = { $secs } s
duration-minutes = { $mins } min { $secs } s
duration-hours = { $hours } h { $mins } min
prompt-exit = ✘ { $code }
prompt-duration-seconds = { $value } s
prompt-duration-minutes = { $minutes } min { $seconds } s
prompt-duration-hours = { $hours } h { $minutes } min

## Tastennamen in Tastenkürzeln

key-ctrl = Strg
key-shift = Umschalt
key-alt = Alt
key-super = Super
key-space = Leertaste
key-backspace = Rücktaste
key-delete = Entf
key-insert = Einfg
key-home = Pos1
key-end = Ende
key-page-up = Bild↑
key-page-down = Bild↓

## Seitenleiste

sidebar-keys = Schlüssel
sidebar-settings = Einstellungen

## Seitenleiste: Einstellungen

settings-language = Sprache
settings-language-auto = Automatisch · { $language }
settings-language-auto-hint = Folgt der Systemsprache (LANG): Deutsch bei deutscher Locale, sonst Englisch
settings-appearance = Darstellung
settings-font-size = Schriftgröße
settings-line-height = Zeilenhöhe
settings-line-height-value = { $factor } ×
settings-padding = Innenabstand
settings-pixels = { $px } px
settings-tab-bar = Tab-Leiste anzeigen
settings-sidebar-width = Breite der Seitenleiste
settings-cursor = Cursor
settings-cursor-blink = Cursor blinkt
settings-cursor-interval = Blinkintervall
settings-milliseconds = { $ms } ms
settings-scroll = Scrollen
settings-scroll-speed = Mausrad
settings-scroll-lines = { $lines ->
    [one] { $lines } Zeile je Raste
   *[other] { $lines } Zeilen je Raste
}
settings-scroll-speed-hint = Standard 3. Touchpads scrollen stufenlos.
settings-scroll-select = Beim Markieren
settings-scroll-select-times = { $factor }× so schnell
settings-scroll-select-off = wie sonst
settings-scroll-select-hint = Solange mit gedrückter Maustaste markiert wird, scrollt das Rad um diesen Faktor weiter – so reicht die Auswahl schnell über mehrere Bildschirme, ohne an den Rand ziehen zu müssen. Standard 3, 1× schaltet es ab.
settings-scrollback = Scrollback
settings-scrollback-lines = { $lines ->
    [one] { $lines } Zeile
   *[other] { $lines } Zeilen
}
settings-scrollback-note = Gilt nach dem Loslassen für alle Tabs; kleiner gestellt, verwirft er die ältesten Zeilen.
settings-integration = Shell-Integration
settings-notify-after = Benachrichtigen nach
settings-notify-never = nie
settings-seconds = { $secs } s
settings-integration-note = Benachrichtigt, wenn ein Befehl so lange lief und sein Tab gerade nicht zu sehen ist. fish, bash und zsh melden Prompts und Verzeichnis in Terminaal von selbst, andere Shells mit OSC 133 und OSC 7.
settings-activity = Tabs im Hintergrund
settings-silence-after = Still nach
settings-activity-note = Ein Tab im Hintergrund bekommt einen Punkt vor dem Titel: in der Akzentfarbe bei neuer Ausgabe, rot bei der Glocke, grün, wenn ein Terminal, auf dessen Stille geachtet wird (Rechtsklick → Auf Stille achten), so lange nichts ausgegeben hat – dann auch mit Benachrichtigung.
paste-warn-title = ⚠ Wirklich einfügen?
paste-warn-broadcast = Geht an { $terminals } Terminals (Broadcast).
paste-warn-runs-lines = { $lines ->
    [one] Die Zeile läuft sofort, wenn sie ankommt – das Programm nimmt Eingefügtes nicht als Ganzes.
   *[other] { $lines } Zeilen, jede läuft sofort, wenn sie ankommt – das Programm nimmt Eingefügtes nicht als Ganzes.
}
paste-warn-lines = { $lines } Zeilen auf einmal.
paste-warn-sudo = Führt etwas mit Root-Rechten aus (sudo, doas, pkexec).
paste-warn-pipe = Gibt etwas einer Shell zum Ausführen, womöglich einen Download (| sh, $(curl …)).
paste-warn-destructive = Kann Dateien oder ganze Datenträger löschen (rm -rf, mkfs, dd).
paste-warn-paste = Einfügen
paste-warn-paste-run = Einfügen und ausführen
paste-warn-hint = Enter fügt ein · Esc bricht ab · abschalten unter Einstellungen → Terminal
paste-warn-more-lines = { $lines ->
    [one] … 1 weitere Zeile
   *[other] … { $lines } weitere Zeilen
}
settings-paste = Einfügen
settings-paste-warning = Vor riskantem Einfügen fragen
settings-paste-warning-hint = Mehrere Zeilen, die sofort laufen würden, sudo, ein Download, der in eine Shell geht, rm -rf und Ähnliches – und mehrere Zeilen, die per Broadcast an mehr als ein Terminal gehen.
settings-shell = Standard-Shell
settings-startup = Beim Start
settings-startup-sidebar = Seitenleiste anzeigen
settings-startup-splash = Startanimation
settings-startup-restore = Letzte Sitzung wiederherstellen
settings-startup-restore-hint = Öffnet die Tabs vom letzten Mal wieder: Aufteilung, Shells samt Verzeichnis, SSH-Verbindungen, Dateien- und Einstellungs-Tab. Kein Scrollback. Ein zweites Terminaal-Fenster startet immer leer.
settings-window-size = Fenstergröße
settings-quake = Dropdown-Fenster
settings-quake-height = Höhe
settings-quake-hide = Ausblenden, wenn ein anderes Fenster den Fokus bekommt
settings-quake-note = Der Befehl blendet ein Terminal am oberen Bildschirmrand ein und wieder aus, beim ersten Mal startet er es. Lege ihn in den Systemeinstellungen als eigenes Tastenkürzel an (COSMIC: Eingabegeräte, Tastatur, Tastenkürzel anzeigen und anpassen, Eigene Tastenkürzel). Das Dropdown-Fenster hat eigene Tabs und eine eigene gespeicherte Sitzung.
settings-quake-copy = Befehl kopieren
settings-window-size-current = Aktuelle übernehmen
settings-window-size-current-hint = Übernimmt die jetzige Fenstergröße ({ $width } × { $height })
settings-startup-note = Wirkt ab dem nächsten Start.
settings-key-hint = config.toml: { $key }
settings-note = Änderungen gelten sofort und werden in ~/.config/terminaal/config.toml gespeichert; Kommentare und Formatierung darin bleiben erhalten. Welcher Schlüssel es ist, zeigt der Tooltip am Namen der Einstellung.
settings-page-general = Allgemein
settings-page-terminal = Terminal
settings-page-shell = Shell
settings-page-shortcuts = Tastenkürzel
settings-font = Schrift
settings-layout = Fenster
settings-shell-aliases-hint = Aliase und Funktionen je Shell: Seitenleiste, Bereich „Shells“.
settings-editor = Editor für Dateien vom Server
settings-editor-default = Standardanwendung (xdg-open)
settings-editor-hint = Befehl, dem der Pfad der lokalen Kopie angehängt wird, z. B. „code“, „gedit“ oder „kate“. Leer: die Standardanwendung des Desktops.
settings-commands-run = Befehle sofort ausführen
settings-commands-run-hint = Aus: der Befehl landet nur in der Eingabezeile, Enter drückst du selbst.
settings-commands-yes = Rückfragen der Befehle überspringen
settings-commands-yes-hint = Hängt --noconfirm bzw. -y an: das Aktualisieren läuft dann ohne Rückfrage durch, auch wenn dabei Pakete ersetzt oder entfernt werden.
settings-commands-system = System
settings-commands-system-auto = Automatisch · { $system }
settings-commands-system-hint = Bestimmt, welche Befehle die Seitenleiste für lokale Tabs anbietet. Für SSH-Hosts steht es im Host-Formular unter „Erweitert“.
settings-theme = Theme
settings-theme-builtin = Eingebautes Theme
settings-theme-own = Eigenes Theme aus { $path }
settings-theme-cosmic = Folgt dem COSMIC-Theme des Desktops, auch beim Wechsel zwischen hell und dunkel
settings-transparency = Transparenz
settings-opacity = Deckkraft
settings-percent = { $value } %
settings-blur = Dahinter unscharf („milchig“)
settings-blur-unsupported = Unschärfe braucht einen Compositor mit ext-background-effect (etwa COSMIC); unter KDE wirkt sie auch so.
settings-transparency-unsupported = Grafiktreiber oder Compositor bieten keine durchscheinenden Fenster an.
settings-transparency-x11 = Unter X11 bleibt das Fenster undurchsichtig; durchscheinend geht es nur unter Wayland.
settings-themes-folder = Eigene Themes: TOML-Dateien in { $path }, im Format von Alacritty (dessen Themes passen unverändert), optional mit [ui] für die Oberfläche. Nach Änderungen „Erneut laden“.
settings-themes-reloaded = { $count ->
    [one] { $count } Theme geladen.
   *[other] { $count } Themes geladen.
}
settings-font-terminal = Konsole
settings-font-ui = Menüs
settings-font-default = Standard · { $font }
settings-font-missing = { $font } (nicht installiert)
settings-font-note = Die Tab-Leiste nutzt die Konsolenschrift.

## Themes

theme-missing-color = Die Farbe { $key } fehlt.
theme-bad-color = { $key }: „{ $value }“ ist keine Farbe (#rrggbb).

## Einstellungen: Tastenkürzel

shortcuts-group-tabs = Tabs
shortcuts-group-panes = Geteilte Ansicht
shortcuts-group-window = Fenster
shortcuts-group-clipboard = Zwischenablage
shortcuts-group-scroll = Scrollen
shortcuts-group-font = Schriftgröße
shortcut-new-tab = Neuer Tab mit der Standard-Shell
shortcut-close-tab = Tab mit allen Bereichen schließen
shortcut-next-tab = Nächster Tab
shortcut-previous-tab = Vorheriger Tab
shortcut-select-tab = Tab { $number }
shortcut-move-tab-left = Tab nach links verschieben
shortcut-move-tab-right = Tab nach rechts verschieben
shortcut-split-right = Rechts teilen
shortcut-split-down = Unten teilen
shortcut-close-pane = Bereich schließen (mit dem letzten den Tab)
shortcut-focus-pane-left = Zum Bereich links
shortcut-focus-pane-right = Zum Bereich rechts
shortcut-focus-pane-up = Zum Bereich oben
shortcut-focus-pane-down = Zum Bereich unten
shortcut-zoom-pane = Bereich vergrößern/wiederherstellen
shortcut-toggle-broadcast = Broadcast an/aus (Eingabe an alle markierten Terminals)
shortcut-open-files = Dateien der SSH-Verbindung öffnen (SFTP)
shortcut-watch-silence = Auf Stille achten an/aus (melden, wenn das Terminal still wird)
shortcut-toggle-sidebar = Seitenleiste ein-/ausblenden
shortcut-open-settings = Einstellungen öffnen
shortcut-command-palette = Befehlspalette (Aktionen, Hosts, Snippets, Tabs, Themes)
palette-placeholder = Aktionen, Hosts, Snippets, Tabs, Themes suchen …
palette-nothing-found = Nichts gefunden.
palette-hint = ↑↓ auswählen · Enter ausführen · Esc schließen
palette-kind-tab = Tab
palette-kind-host = Host
palette-kind-snippet = Snippet
palette-kind-theme = Theme
palette-kind-shell = Shell
palette-tab-number = Tab { $number }
palette-theme-current = aktuell
palette-new-tab = Neuer Tab: { $shell }
shortcut-copy = Kopieren
shortcut-copy-last-output = Ausgabe des letzten Befehls kopieren
shortcut-paste = Einfügen
shortcut-paste-and-run = Einfügen und ausführen
shortcut-scroll-page-up = Eine Seite zurück
shortcut-scroll-page-down = Eine Seite vor
shortcut-scroll-to-top = Zum Anfang des Scrollbacks
shortcut-scroll-to-bottom = Zum Ende
shortcut-search = Im Scrollback suchen
shortcut-previous-prompt = Zum vorigen Prompt
shortcut-next-prompt = Zum nächsten Prompt
shortcut-font-bigger = Größer
shortcut-font-smaller = Kleiner
shortcut-font-reset = Zurücksetzen
shortcuts-none = nicht belegt
shortcuts-add-hint = Weitere Tastenkombination aufnehmen
shortcuts-press = Tasten drücken …
shortcuts-press-hint = Esc oder ein Klick hier bricht ab
shortcuts-remove-hint = { $combo } entfernen
shortcuts-reset-hint = Zurück auf den Standard: { $combos }
shortcuts-shadowed = Auch „{ $action }“ zugeordnet – dort gilt sie.
shortcuts-taken = { $combo } ist schon „{ $action }“ zugeordnet und müsste dort erst entfernt werden.
shortcuts-swallows-typing = { $combo } würde normale Eingaben abfangen. Möglich sind Kombinationen mit Strg, Alt oder Super, F-Tasten sowie Umschalt mit Bild↑/↓, Pos1, Ende, Einfg, Entf oder Pfeiltasten.
shortcuts-key-hint = config.toml: [shortcuts] { $key }
shortcuts-font-note = Gilt bis zum Beenden; die gespeicherte Größe steht unter Darstellung.
shortcuts-note = Änderungen gelten sofort und werden unter [shortcuts] in ~/.config/terminaal/config.toml gespeichert. Scrollen per Tastatur wirkt nicht in Vollbildprogrammen wie less oder vim – dort geht die Taste an das Programm.

## Seitenleiste: Shells, Aliase und Funktionen

shells-installed = Installierte Shells
shells-double-click = Doppelklick öffnet einen neuen Tab
shells-new-tab = ▶  Neuer Tab
shells-new-tab-hint = Öffnet einen neuen Tab mit { $shell }
shells-make-default = ★  Als Standard
shells-make-default-hint = Neue Tabs (Strg+Umschalt+T, „+“) starten mit dieser Shell
shells-managed-title = Aliase, Funktionen & Zeilen · { $shell }
shells-managed-unsupported = Für { $shell } kann Terminaal nichts verwalten – unterstützt werden fish, bash und zsh.
shells-managed-file-hint = Wird nur in Terminaal geladen, nicht in anderen Terminals. Änderungen gelten für neu geöffnete Tabs.
shells-save-failed = Speichern fehlgeschlagen: { $err }
shells-aliases = Aliase ({ $count })
shells-functions = Funktionen ({ $count })
shells-no-aliases = Noch keine Aliase angelegt.
shells-no-functions = Noch keine Funktionen angelegt.
shells-add-alias = +  Alias hinzufügen
shells-add-function = +  Funktion hinzufügen
shells-new-alias = Neuer Alias
shells-edit-alias = Alias bearbeiten
shells-new-function = Neue Funktion
shells-edit-function = Funktion bearbeiten
shells-command = Befehl
shells-body = Rumpf
shells-args-fish = Argumente stehen in $argv.
shells-args-posix = Argumente: $1, $2, … bzw. "$@".
shells-no-entry-open = Kein Eintrag geöffnet.
shells-command-missing = Der Befehl fehlt.
shells-body-missing = Der Rumpf fehlt.
shells-name-taken = „{ $name }“ ist bereits als { $kind } definiert.
shells-saved = „{ $name }“ gespeichert – gilt für neue { $shell }-Tabs.
shells-lines = Zeilen ({ $count })
shells-no-lines = Noch keine Zeilen angelegt.
shells-add-lines = +  Zeilen hinzufügen
shells-new-lines = Neue Zeilen
shells-edit-lines = Zeilen bearbeiten
shells-lines-label = Zeilen
shells-lines-hint = Läuft beim Start jedes Terminaal-Tabs mit dieser Shell – wie ein Eintrag in der Startdatei der Shell, nur in Terminaals eigener Datei.
shells-lines-missing = Die Zeilen fehlen.

## Seitenleiste: Befehle (commands.rs)

cmd-title = Befehle
cmd-broadcast = Broadcast: geht an { $count } Terminals
snip-title = Eigene Befehle
snip-manage = Verwalten
snip-manage-done = Fertig
snip-none = Noch keine eigenen Befehle – „Verwalten“ legt welche an.
snip-none-here = Keine eigenen Befehle für dieses System oder diesen Host.
snip-add = +  Befehl hinzufügen
snip-new = Neuer Befehl
snip-edit = Befehl bearbeiten
snip-name-hint = z. B. Logs verfolgen
snip-command-note = Mehrere Zeilen kommen zusammen an, wie eingefügt.
snip-system = Nur auf System
snip-all-systems = Alle Systeme
snip-host = Wo
snip-autorun = Automatisch ausführen
snip-autorun-never = Nie, nur als Knopf
snip-autorun-shell = Mit der Shell
snip-autorun-login = Nach dem Login
snip-autorun-note = „Mit der Shell“: in jedem neuen Terminal, sobald die Shell bereit ist, lokal und über SSH. „Nach dem Login“: nur in SSH-Terminals nach der Anmeldung. Beides auch nach einem Wiederverbinden, nur wo System und Host passen, ohne Rückfrage. Der Knopf bleibt.
snip-hidden = Knopf ausblenden
snip-hidden-hint = Kein Knopf unter den Befehlen; der Befehl steht nur unter „Verwalten“ und läuft von dort oder automatisch.
snip-hidden-short = ausgeblendet
snip-run = Ausführen
snip-all-hidden = { $count ->
    [one] Ein ausgeblendeter Befehl – unter „Verwalten“.
   *[other] { $count } ausgeblendete Befehle – unter „Verwalten“.
}
snip-all-hosts = Überall (lokal und alle Hosts)
snip-local-only = Nur lokal
snip-local-login = „Nach dem Login“ gilt nur für SSH-Terminals und passt nicht zu „Nur lokal“.
snip-everywhere = Überall
snip-on-host = auf { $host }
snip-command-missing = Der Befehl fehlt.
snip-category = Kategorie
snip-category-hint = optional, z. B. Docker
snip-category-use = Diese Kategorie übernehmen
snip-name-taken = Einen Befehl „{ $name }“ gibt es schon.
cmd-system = System: { $system }
cmd-system-local-hint = Aus /etc/os-release erkannt. Stimmt das nicht, lässt es sich in den Einstellungen unter „Shell“ festlegen.
cmd-system-remote-hint = Beim Verbinden auf { $host } ermittelt. Stimmt das nicht, lässt es sich im Host-Formular unter „Erweitert“ festlegen.
cmd-system-detect = System wird wieder automatisch erkannt.
cmd-system-probing = System wird ermittelt …
cmd-system-unknown = System nicht erkannt – die Paketbefehle bleiben aus. In den Einstellungen unter „Shell“ lässt es sich festlegen.
cmd-no-tab = Kein Terminal-Tab offen – Befehle brauchen eine Shell.
cmd-run-hint = Führt sofort aus: { $line }
cmd-type-hint = Schreibt in die Eingabezeile: { $line }
cmd-warn-title = Befehle laufen sofort los
cmd-warn-body = Ein Klick schickt den Befehl direkt an die Shell des aktiven Tabs, samt Enter. In den Einstellungen unter „Shell“ lässt sich das auf bloßes Eintippen umstellen.
cmd-warn-run = Verstanden, ausführen
cmd-group-packages = Pakete
cmd-group-disk = Speicherplatz
cmd-group-system = System
cmd-group-network = Netzwerk
cmd-group-collapse = Gruppe einklappen
cmd-group-expand = Gruppe ausklappen
cmd-update = System aktualisieren
cmd-update-flatpak = Flatpaks aktualisieren
cmd-update-aur = AUR-Pakete aktualisieren
cmd-outdated = Verfügbare Updates
cmd-disk-free = Freier Speicher
cmd-disk-usage = Ordnergrößen hier
cmd-memory = Arbeitsspeicher
cmd-processes = Top-Prozesse
cmd-uptime = Laufzeit
cmd-failed-services = Fehlgeschlagene Dienste
cmd-log-errors = Fehler im Log
cmd-ports = Offene Ports
cmd-addresses = IP-Adressen
cmd-family-arch = Arch, CachyOS, Manjaro (pacman)
cmd-family-debian = Debian, Ubuntu, Mint (apt)
cmd-family-fedora = Fedora, RHEL, Rocky (dnf)
cmd-family-suse = openSUSE, SLES (zypper)
cmd-family-alpine = Alpine (apk)
cmd-family-void = Void (xbps)
cmd-family-gentoo = Gentoo (emerge)
cmd-family-nixos = NixOS (nixos-rebuild)
cmd-family-macos = macOS (brew)
cmd-family-freebsd = FreeBSD (pkg)
cmd-family-unknown = Automatisch erkennen

## Verwaltete Alias-/Funktionsdateien (shells/managed.rs)

managed-alias = Alias
managed-function = Funktion
managed-block = Zeilen
managed-unsupported = diese Shell wird nicht unterstützt
managed-invalid-name = Erlaubt sind Buchstaben, Ziffern und _ . : + - (nicht am Anfang: -).
managed-marker-line = Eine Zeile darf nicht mit „# terminaal:“ beginnen – das sind die Markierungen der Datei.
managed-header =
    # Aliase, Funktionen und eigene Zeilen für Terminaal.
    #
    # Wird von Terminaal verwaltet und nur in Terminaal geladen. Die Blöcke
    # zwischen den Markierungen dürfen auch von Hand bearbeitet werden;
    # alles außerhalb davon geht beim nächsten Speichern aus der Seitenleiste
    # verloren.

## Seitenleiste: SSH-Hosts

ssh-store-unreadable = { $file } ist unlesbar – gespeichert wird erst nach „Erneut laden“.
ssh-auto-key = Automatisch (Agent, ~/.ssh/id_*)
ssh-files-key = IdentityFile (siehe Erweitert)
ssh-saved-hosts = Gespeicherte Hosts
ssh-double-click-connects = Doppelklick verbindet
ssh-no-hosts = Noch keine Hosts gespeichert.
ssh-add-host = +  Host hinzufügen
ssh-from-config = Aus ~/.ssh/config
ssh-connect = ▶  Verbinden
ssh-connect-as = Verbinden als { $login }
ssh-adopt = Übernehmen
ssh-adopt-hint = Als gespeicherten Host übernehmen und bearbeiten
ssh-secrets-note = Passwörter und Passphrasen werden nie gespeichert, sondern im Tab abgefragt. Host-Keys werden gegen ~/.ssh/known_hosts geprüft.
ssh-via = { $address } · über { $jump }
ssh-more-logins =
    { $count ->
        [one] +1 Login
       *[other] +{ $count } Logins
    }
ssh-is-jump-host = „{ $name }“ ist Jump-Host von { $hosts } – erst dort ändern.

## Seitenleiste: Host-Formular

host-new = Neuer Host
host-edit = Host bearbeiten
host-host-hint = server.example.com oder 192.168.1.10
host-name-optional = Name (optional)
host-name-hint = wie der Host
host-jump = Jump-Host (ProxyJump)
host-jump-none = Keiner
host-no-host-open = Kein Host geöffnet.
host-missing = Der Host fehlt.
host-has-spaces = Der Host darf keine Leerzeichen enthalten.
host-bad-port = Der Port muss eine Zahl zwischen 1 und 65535 sein.
host-name-taken = Es gibt schon einen Host namens „{ $name }“.
host-bad-user = Ein Benutzername darf weder Leerzeichen noch @ enthalten.
host-login-twice = Die Anmeldung „{ $login }“ steht doppelt drin.
host-jumps-itself = Ein Host kann nicht über sich selbst springen.
host-jump-and-proxy = Jump-Host und ProxyCommand schließen sich aus – eins von beiden leeren.
host-not-a-number = { $label }: „{ $value }“ ist keine ganze Zahl.
host-login = Anmeldung
host-login-more = Weitere { $index }
host-remove-login = Anmeldung entfernen
host-make-default-login = Als Standard – Doppelklick und Jump-Host melden sich so an
host-user-hint = Benutzer (sonst { $user })
host-add-login = +  Weitere Anmeldung
host-add-login-hint = Anderer Benutzer oder Schlüssel für denselben Host
host-keys-hint = Eigene Schlüssel legst du im Bereich „Schlüssel“ an.
host-forwards =
    { $count ->
        [0] Portweiterleitungen
       *[other] Portweiterleitungen ({ $count })
    }
host-forward-local = Lokal
host-forward-remote = Remote
host-forward-dynamic = SOCKS
host-forward-remote-dynamic = SOCKS auf Server
host-forward-local-hint = LocalForward: ein Port oder Socket hier führt zu einem Ziel, das der Server erreicht
host-forward-remote-hint = RemoteForward: ein Port auf dem Server führt zu einem Ziel, das dieser Rechner erreicht
host-forward-dynamic-hint = DynamicForward: ein SOCKS-Proxy hier – Programme sagen selbst, wohin, und die Verbindung geht vom Server aus weiter
host-forward-remote-dynamic-hint = RemoteForward ohne Ziel: ein SOCKS-Proxy auf dem Server – die Verbindungen gehen von diesem Rechner aus weiter
host-remove-forward = Weiterleitung entfernen
host-local-listen-hint = Port hier, z. B. 8080, oder Socket-Pfad
host-local-target-hint = Ziel vom Server aus, z. B. localhost:5432 oder /run/app.sock
host-remote-listen-hint = Port auf dem Server, z. B. 9000
host-remote-target-hint = Ziel von hier aus, z. B. localhost:3000 oder ~/app.sock
host-dynamic-listen-hint = Port hier für den Proxy, z. B. 1080
host-remote-dynamic-listen-hint = Port auf dem Server für den Proxy, z. B. 1080
host-add-forward = +  Weiterleitung
host-forwards-note = Aktiv, solange der Tab verbunden ist. Vor dem Port kann eine Adresse stehen, z. B. *:8080 für alle Netzwerkschnittstellen. Pfade mit / sind Unix-Sockets – der Server kann allerdings nur an Ports lauschen.
host-forward-missing-port = Bei Weiterleitung { $row } fehlt der Port bzw. Socket-Pfad.
host-forward-missing-target = Bei Weiterleitung { $row } fehlt das Ziel.
host-forward-invalid = Weiterleitung { $row }: { $err }
host-advanced =
    { $count ->
        [0] Erweitert
       *[other] Erweitert ({ $count } gesetzt)
    }

## Seitenleiste: Host-Formular, erweiterte Optionen

adv-connection = Verbindung
adv-connect-timeout = Verbindungs-Timeout in s
adv-connect-timeout-name = Verbindungs-Timeout
adv-alive-interval = Keepalive alle … s (0 = aus)
adv-alive-interval-name = Keepalive-Intervall
adv-alive-count = Abbruch nach … Keepalives ohne Antwort
adv-alive-count-name = Keepalives ohne Antwort
adv-compression = Kompression
adv-compression-hint = Compression – hilft bei langsamen Leitungen
adv-address-family = Adressfamilie
adv-proxy-command = Proxy-Befehl statt TCP
adv-proxy-command-hint = z. B. nc -X connect -x proxy:3128 %h %p
adv-auth = Anmeldung
adv-identity-files = Schlüsseldateien, je Zeile eine
adv-identity-files-note = Gelten für Anmeldungen mit „Automatisch“ als Schlüssel.
adv-identities-only = Nur konfigurierte Schlüssel anbieten
adv-identities-only-hint = IdentitiesOnly – keine weiteren Schlüssel aus dem Agent
adv-agent-socket = Agent-Socket
adv-agent-socket-hint = $SSH_AUTH_SOCK · none = ohne Agent
adv-forward-agent = Agent an den Server weiterleiten
adv-forward-agent-hint = ForwardAgent – Programme auf dem Server können mit den Schlüsseln des Agents signieren, solange die Verbindung steht. Nur bei vertrauenswürdigen Servern. Weitergeleitet wird der Agent-Socket darüber, sonst $SSH_AUTH_SOCK.
adv-forward-agent-socket = Weitergeleitet wird { $socket }
adv-methods = Methoden in dieser Reihenfolge
adv-host-key = Host-Key
adv-unknown-host-keys = Unbekannte Host-Keys
adv-changed-host-key-note = Ein geänderter Host-Key bricht die Verbindung immer ab. Den gespeicherten zeigt und entfernt 🔑 Host-Key beim ausgewählten Host.
adv-known-hosts-file = known_hosts-Datei
adv-session = Sitzung
adv-remote-command = Befehl statt Login-Shell
adv-remote-command-hint = z. B. tmux new -A -s main
adv-set-env = Umgebungsvariablen, je Zeile NAME=Wert
adv-set-env-hint = LANG=de_DE.UTF-8
adv-send-env = Lokale Variablen mitgeben
adv-env-note = Der Server übernimmt nur, was seine AcceptEnv-Liste erlaubt. TERM=… setzt den Terminaltyp.
adv-system = System (für die Befehle)
adv-system-hint = Bestimmt, welche Befehle die Seitenleiste im Bereich „Shells“ anbietet – Paketmanager und Dienste unterscheiden sich je System. Automatisch: beim Verbinden ermittelt. Eigene Option von Terminaal, kein ssh_config-Schlüsselwort.
adv-look = Aussehen
adv-color = Warnfarbe
adv-color-hint = Markiert die Tabs dieses Hosts mit einer farbigen Linie und seine Terminals mit einem Rahmen – z. B. Rot für Produktionsserver. Eigene Option von Terminaal, kein ssh_config-Schlüsselwort.
adv-color-none = Keine
adv-color-red = Rot
adv-color-orange = Orange
adv-color-yellow = Gelb
adv-color-green = Grün
adv-color-blue = Blau
adv-color-purple = Lila
adv-color-custom = Eigene
adv-theme = Theme
adv-theme-hint = Die Konsolenfarben der Terminals dieses Hosts; der Rest des Fensters behält sein Theme. Eigene Option von Terminaal, kein ssh_config-Schlüsselwort.
adv-theme-window = Wie das Fenster
adv-look-note = Gilt für danach geöffnete Terminals.
adv-algorithms = Algorithmen
adv-kex = Schlüsseltausch
adv-host-key-types = Host-Key-Typen
adv-ciphers = Verschlüsselung
adv-macs = Integrität
adv-algorithms-note = Leer = Standard. +liste ergänzt, -liste entfernt, ^liste stellt nach vorn; * und ? sind erlaubt.

## SSH-Optionen (ssh/options.rs)

opt-family-any = IPv4 und IPv6
opt-family-inet = Nur IPv4
opt-family-inet6 = Nur IPv6
opt-check-ask = Nachfragen (ask)
opt-check-accept-new = Neue ohne Nachfrage speichern (accept-new)
opt-check-yes = Nur bekannte Hosts (yes)
opt-unknown-method = PreferredAuthentications: unbekannte Methode „{ $name }“.
opt-no-method = PreferredAuthentications nennt keine Methode, die Terminaal kann (publickey, keyboard-interactive, password).
opt-no-algorithm = keiner der Algorithmen „{ $spec }“ wird unterstützt (möglich: { $supported })
opt-list-empty = { $keyword }: die Liste ist leer.
opt-bad-color = color: „{ $value }“ ist weder red, orange, yellow, green, blue, purple noch #rrggbb.
opt-forward-invalid = { $keyword } „{ $spec }“: { $reason }
opt-forward-remote-unix = auf einem Unix-Socket kann der Server mit libssh2 nicht lauschen, nur an einem Port
opt-forward-local-no-target = ohne Ziel geht das nur als DynamicForward (SOCKS)
opt-forward-syntax = erwartet wird [Adresse:]Port oder Socket-Pfad, dann Ziel:Port oder Socket-Pfad
opt-forward-dynamic-syntax = erwartet wird [Adresse:]Port
opt-forward-port = ungültiger Port
opt-forward-port-zero = Port 0 geht nur bei RemoteForward
opt-forward-target-port = ungültiger Zielport
opt-forward-target-missing = das Ziel fehlt
opt-setenv-syntax = SetEnv „{ $entry }“: erwartet wird NAME=Wert.
opt-setenv-name = SetEnv „{ $entry }“: „{ $name }“ ist kein gültiger Variablenname.

## Hosts auflösen (ssh/mod.rs)

catalog-jump-and-proxy = „{ $name }“ hat ProxyJump und ProxyCommand – es geht nur eins von beiden.
catalog-jump-loop = Die ProxyJump-Kette über „{ $name }“ ist zu lang oder führt im Kreis.
catalog-key-gone = Den Schlüssel „{ $key }“ von „{ $host }“ gibt es nicht mehr.
catalog-bad-jump = Ungültiger Jump-Host „{ $spec }“ – erwartet wird ein Hostname aus der Liste oder [user@]host[:port].

## Schlüsselspeicher (ssh/keys.rs)

keystore-public-invalid = Der öffentliche Schlüssel von „{ $name }“ ist ungültig: { $err }
keystore-no-source = „{ $name }“ hat weder eine Datei noch einen Agent-Schlüssel.
keystore-unreadable-file = { $path } ist kein lesbarer OpenSSH-Schlüssel: { $err }
keystore-generate-failed = Schlüssel konnte nicht erzeugt werden: { $err }
keystore-exists = { $path } gibt es schon.
keystore-delete-failed = { $path } konnte nicht gelöscht werden: { $err }
keystore-no-agent = Kein SSH-Agent gefunden ($SSH_AUTH_SOCK ist nicht gesetzt).
keystore-agent-unreachable = SSH-Agent nicht erreichbar: { $err }

## Seitenleiste: Schlüssel

keys-unreadable = nicht lesbar
keys-badge-agent = Agent
keys-badge-file = Datei
keys-click-details = Klick zeigt die Details
keys-none = Noch keine Schlüssel angelegt.
keys-generate = +  Neu erzeugen
keys-add-file = +  Datei
keys-add-file-hint = Vorhandene Schlüsseldatei hinzufügen
keys-from-agent = +  Aus Agent
keys-from-agent-hint = Schlüssel aus dem SSH-Agent übernehmen, z. B. 1Password
keys-note = Neue Schlüssel: Ed25519 im OpenSSH-Format. Passphrasen werden nie gespeichert. „Entfernen“ löscht den Eintrag; die Dateien hier erzeugter Schlüssel auf Nachfrage mit, andere Schlüsseldateien nie.
keys-unknown = unbekannt
keys-type = Typ: { $algorithm }
keys-comment-line = Kommentar: { $comment }
keys-file-line = Datei: { $file }
keys-agent-only = Liegt nur im SSH-Agent.
keys-used-by = Verwendet von: { $hosts }
keys-delete-files-question = „{ $name }“ wird aus Terminaal entfernt. Auch die Schlüsseldateien löschen?
keys-delete-files = Dateien löschen
keys-keep-files = Dateien behalten
keys-copy-public = 📋  Öffentlichen Schlüssel kopieren
keys-rename = Umbenennen
keys-copied = Öffentlicher Schlüssel von „{ $name }“ kopiert – auf dem Server in ~/.ssh/authorized_keys eintragen.
keys-still-used = „{ $name }“ wird noch von { $hosts } verwendet.
keys-removed-with-files = „{ $name }“ entfernt, die Schlüsseldateien sind gelöscht.
keys-removed-but = „{ $name }“ entfernt, aber { $err }
keys-removed-file-kept = „{ $name }“ entfernt – die Schlüsseldatei selbst bleibt erhalten.
keys-removed = „{ $name }“ entfernt.
keys-new-title = Neuer Schlüssel (Ed25519)
keys-name-hint = Arbeit
keys-comment = Kommentar
keys-location = Speicherort
keys-passphrase-optional = Passphrase (optional)
keys-passphrase-repeat = Passphrase wiederholen
keys-no-passphrase-note = Ohne Passphrase liegt der Schlüssel unverschlüsselt auf der Platte.
keys-add-file-title = Schlüsseldatei hinzufügen
keys-private-file = Private Schlüsseldatei
keys-agent-title = Schlüssel aus dem SSH-Agent
keys-agent-empty = Der Agent hat keine Schlüssel.
keys-no-comment = (ohne Kommentar)
keys-rename-title = Schlüssel umbenennen
keys-new-name = Neuer Name
keys-rename-note = Hosts, die den Schlüssel verwenden, werden mit umgestellt.
keys-no-form = Kein Formular offen.
keys-passphrase-mismatch = Die Passphrasen stimmen nicht überein.
keys-location-missing = Der Speicherort fehlt.
keys-generated = „{ $name }“ erzeugt ({ $path }). Den öffentlichen Schlüssel auf dem Server in ~/.ssh/authorized_keys eintragen.
keys-file-missing = Die Datei fehlt.
keys-added = „{ $name }“ hinzugefügt.
keys-choose-from-list = Bitte einen Schlüssel aus der Liste wählen.
keys-agent-duplicate = Dieser Schlüssel ist schon als „{ $name }“ angelegt.
keys-taken-over = „{ $name }“ übernommen.
keys-gone = Den Schlüssel gibt es nicht mehr.
keys-renamed = „{ $old }“ heißt jetzt „{ $new }“.
keys-name-taken = Es gibt schon einen Schlüssel namens „{ $name }“.

## SSH-Verbindung, Ausgaben im Tab (ssh/connection.rs)

conn-ssh-error = SSH-Fehler: { $err }
conn-io-error = Ein-/Ausgabefehler: { $err }
conn-retry-in = Neuer Versuch in { $secs } s – Enter: jetzt versuchen · Strg+D: Tab schließen
conn-retry-or-close = Enter: erneut versuchen · Strg+D: Tab schließen
conn-needs-input = Die Anmeldung braucht eine Eingabe – ein automatischer Versuch kann sie nicht beantworten.
conn-reconnecting = Verbinde erneut mit { $target } …
conn-reconnecting-auto = Verbinde erneut mit { $target } (automatisch) …
conn-connecting = Verbinde mit { $target } …
conn-connecting-via = Verbinde mit { $target } über { $hops } …
conn-tunnel-failed = { $hop } konnte keinen Tunnel zu { $host }:{ $port } öffnen: { $err }
conn-handshake-failed = SSH-Handshake mit { $hop } fehlgeschlagen: { $err }
conn-proxy-says = ProxyCommand meldet: { $output }
conn-session-closed = Terminaal: Sitzung beendet
conn-tunnel-closed = Terminaal: Tunnel beendet
conn-resolve-failed = { $host } konnte nicht aufgelöst werden: { $err }
conn-connect-failed = Verbindung zu { $host }:{ $port } fehlgeschlagen: { $err }
conn-no-address = Für { $host } wurde keine Adresse gefunden ({ $family }).
conn-proxy-start-failed = ProxyCommand „{ $command }“ konnte nicht gestartet werden: { $err }
conn-no-host-key = Der Server hat keinen Host-Key geschickt.
conn-unknown-type = unbekannter
conn-host-key-changed =
    WARNUNG: Der Host-Key von { $entry } hat sich geändert!
    Das kann ein Angriff sein (Man-in-the-Middle) – oder der Server wurde neu aufgesetzt.
    Gespeichert in { $file }:
    { $stored }
    Neu vom Server: { $kind } { $fingerprint }
    Verbindung abgebrochen. Ist die Änderung erwartet, entferne den alten Eintrag
    in der Seitenleiste (SSH → Host auswählen → Host-Key) oder mit
      ssh-keygen -R '{ $entry }'{ $file_option }
conn-host-key-refused = Der Host-Key von „{ $entry }“ steht nicht in { $file }, und StrictHostKeyChecking lässt nur bekannte Hosts zu.
    { $kind }-Fingerprint: { $fingerprint }
conn-host-key-question =
    Die Echtheit von „{ $entry }“ kann nicht bestätigt werden.
    { $kind }-Fingerprint: { $fingerprint }
    Verbinden und den Host-Key in { $file } speichern? (ja/nein):
conn-host-key-rejected = Abgebrochen: Host-Key nicht bestätigt.
conn-known-hosts-failed = { $file } konnte nicht ergänzt werden: { $err }
conn-host-key-saved = Host-Key von „{ $entry }“ ({ $kind }, { $fingerprint }) in { $file } gespeichert.
known-hosts-changed = Die Datei hat sich inzwischen geändert – bitte neu laden.
conn-stored-key = { $kind } { $fingerprint } (Zeile { $line })
known-hosts-button = 🔑 Host-Key
known-hosts-button-hint = Den gespeicherten Host-Key aus known_hosts anzeigen oder entfernen
known-hosts-title = Host-Key von { $entry }
known-hosts-none = Kein Eintrag gespeichert – beim nächsten Verbinden wird der Host-Key je nach StrictHostKeyChecking erfragt.
known-hosts-line = Zeile { $line }
known-hosts-hashed = Name gehasht
known-hosts-also = gilt für { $hosts }
known-hosts-remove =
    { $count ->
        [one] Eintrag entfernen
       *[other] { $count } Einträge entfernen
    }
known-hosts-confirm = Nur entfernen, wenn feststeht, dass der Server einen neuen Schlüssel hat – sonst kann es ein Angriff sein. Die ganze Zeile fällt weg, auch für die anderen Namen darin. Beim nächsten Verbinden wird der neue Host-Key zur Bestätigung angezeigt.
known-hosts-removed = Host-Key von { $entry } aus { $file } entfernt.
known-hosts-failed = { $file } konnte nicht geändert werden: { $err }
known-hosts-no-file = Kein known_hosts-Pfad ($HOME ist nicht gesetzt).
conn-denied = Zugriff verweigert.
conn-password-prompt = Passwort für { $target }:
conn-auth-failed = Anmeldung als { $user } bei { $host } fehlgeschlagen (vom Server angeboten: { $methods }).
conn-auth-failed-preferred = Anmeldung als { $user } bei { $host } fehlgeschlagen (vom Server angeboten: { $methods }; PreferredAuthentications: { $preferred }).
conn-max-auth-tries = Der SSH-Agent hat { $count } Schlüssel durchprobiert – viele Server brechen nach wenigen Versuchen ab (MaxAuthTries). Abhilfe: dem Host im Bereich „SSH“ einen festen Schlüssel zuweisen.
conn-key-file-missing = Schlüsseldatei { $path } nicht gefunden.
conn-wrong-passphrase = Falsche Passphrase.
conn-passphrase-prompt = Passphrase für { $path }:
conn-keepalive-dead = Verbindung verloren: { $target } antwortet nicht mehr ({ $count } Keepalives ohne Antwort).
conn-lost = Verbindung verloren: { $err }
conn-agent-forward-up = Agent-Weiterleitung über { $socket }
conn-agent-forward-failed = Agent-Weiterleitung nicht möglich: { $err }
conn-agent-forward-refused = der Server lässt sie nicht zu (AllowAgentForwarding)
conn-agent-forward-unset = { $var } ist nicht gesetzt
conn-agent-forward-off = IdentityAgent ist none
conn-agent-forward-missing = kein Agent unter { $socket }
conn-algorithms-failed = { $keyword } für { $hop }: { $err }
conn-unknown-key-type = unbekannter Schlüsseltyp

## Portweiterleitungen (ssh/forward.rs)

forward-up = Weiterleitung { $forward }
forward-failed = Weiterleitung { $forward } fehlgeschlagen: { $err }
forward-no-address = keine Adresse gefunden

## Weiterleitungen des aktiven Tabs (ui/ssh_panel.rs)

fwd-title = Weiterleitungen · { $tab }
fwd-active = aktiv
fwd-starting = wird eingerichtet …
fwd-paused = angehalten
fwd-failed = fehlgeschlagen: { $err }
fwd-pause = Anhalten
fwd-start = Starten
fwd-retry = Erneut versuchen
fwd-remote-paused-note = Eine angehaltene Remote-Weiterleitung lauscht auf dem Server weiter, weist Verbindungen aber ab.

## Dateien einer SSH-Verbindung (sftp/, ui/files_panel.rs)

files-tab = Dateien: { $host }
files-tab-attention = ⚠ Dateien: { $host }
files-title = Dateien auf { $host }
files-connect-failed = SFTP nicht verfügbar: { $err }. Ist die Anmeldung im Terminal schon durch?
files-reconnect = Neu verbinden
files-waiting-login = Warte auf die Anmeldung im Terminal …
files-waiting-reconnect = Verbindung unterbrochen – es geht weiter, sobald das Terminal wieder verbunden ist.
files-terminal-closed = Das Terminal dieser Verbindung ist geschlossen. Öffne den Host erneut – der Dateien-Tab übernimmt die neue Verbindung.
files-offline = Gerade nicht verbunden.
files-reconnected = Das Terminal hat neu verbunden.
files-stream-closed = SFTP-Kanal geschlossen.
files-transfer-waiting = wartet auf die Verbindung
files-local = Dieser Rechner
files-remote = Server { $host }
files-up = Ordner darüber
files-home = Home-Ordner
files-reload = Neu laden
files-upload = Hochladen
files-download = Herunterladen
files-edit = Bearbeiten
files-edit-hint = Lokal im Editor öffnen; jedes Speichern geht zurück auf den Server
files-rename = Umbenennen
files-rename-to = „{ $name }“ umbenennen in:
files-new-folder = Neuer Ordner
files-new-folder-name = Name des neuen Ordners:
files-delete = Löschen
files-delete-confirm = Wirklich löschen?
files-delete-hint = Dateien und leere Ordner; ein zweiter Klick löscht
files-ok = OK
files-cancel = Abbrechen
files-created = „{ $name }“ angelegt.
files-renamed = Umbenannt in „{ $name }“.
files-removed = „{ $name }“ gelöscht.
files-list-failed = { $dir } lässt sich nicht öffnen: { $err }
files-local-failed = { $path }: { $err }
files-transfers = Übertragungen
files-clear-transfers = Erledigte entfernen
files-cancel-transfer = Abbrechen
files-transfer-done = fertig
files-transfer-cancelled = abgebrochen
files-edits = Lokal bearbeitet
files-edits-note = Lokale Kopien liegen in einem privaten Ordner und werden beim Schließen gelöscht. Vor dem Hochladen wird geprüft, ob die Datei auf dem Server inzwischen geändert wurde.
files-edit-local = Lokale Kopie: { $path }
files-edit-synced = auf dem Server
files-edit-uploading = wird hochgeladen …
files-edit-conflict-state = auf dem Server inzwischen geändert – nichts überschrieben
files-edit-denied-read = keine Leserechte
files-edit-denied-write = keine Schreibrechte – Änderung noch nicht auf dem Server
files-edit-overwrite = Überschreiben
files-edit-overwrite-hint = Die lokale Fassung ersetzt die geänderte Datei auf dem Server
files-edit-take-theirs = Server-Fassung übernehmen
files-edit-take-theirs-hint = Die lokalen Änderungen verwerfen und die Datei vom Server neu laden
files-edit-reopen = Im Editor öffnen
files-edit-close = Schließen
files-edit-close-unsaved = Änderungen verwerfen?
files-edit-close-hint = Die lokale Kopie löschen
files-edit-saved = „{ $name }“ auf dem Server gespeichert.
files-edit-conflict = „{ $name }“ wurde auf dem Server geändert, während du es bearbeitet hast. Nichts wurde überschrieben.
files-edit-failed = „{ $name }“ lässt sich nicht bearbeiten: { $err }
files-edit-not-a-file = „{ $name }“ ist keine gewöhnliche Datei.
files-edit-too-big = „{ $name }“ ist zu groß zum Bearbeiten (mehr als { $limit } MB).
files-edit-upload-failed = Hochladen fehlgeschlagen: { $err }
files-sudo-use = Mit sudo …
files-sudo-use-hint = Legt ein kleines Skript auf dem Server ab, das du im Terminal ausführst – sudo fragt dort wie gewohnt nach dem Passwort
files-sudo-ready = mit sudo im Terminal ausführen:
files-sudo-run = Im Terminal ausführen
files-sudo-run-hint = Fügt den Befehl ins Terminal dieser Verbindung ein und führt ihn aus
files-sudo-no-terminal = Das Terminal dieser Verbindung ist geschlossen
files-sudo-running = warte auf sudo im Terminal …
files-sudo-failed = sudo ist fehlgeschlagen (Exit-Code { $code }).
files-sudo-prepare-failed = Das sudo-Skript lässt sich nicht anlegen: { $err }
files-sudo-path = Der Home-Ordner { $path } enthält Zeichen, die sich nicht sicher in einen Befehl einfügen lassen.
files-sudo-reading = Terminaal: { $path } mit sudo lesen
files-sudo-writing = Terminaal: { $path } mit sudo schreiben
