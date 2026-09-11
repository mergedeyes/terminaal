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

cli-usage = Aufruf: terminaal [--connect [BENUTZER@]HOST]
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
menu-paste = Einfügen
menu-paste-run = Einfügen und ausführen
menu-shortcut-copy = Strg+Umschalt+C
menu-shortcut-paste = Strg+Umschalt+V

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
settings-scrollback = Scrollback
settings-scrollback-lines = { $lines ->
    [one] { $lines } Zeile
   *[other] { $lines } Zeilen
}
settings-scrollback-note = Gilt nach dem Loslassen für alle Tabs; kleiner gestellt, verwirft er die ältesten Zeilen.
settings-shell = Standard-Shell
settings-startup = Beim Start
settings-startup-sidebar = Seitenleiste anzeigen
settings-startup-splash = Startanimation
settings-window-size = Fenstergröße
settings-window-size-current = Aktuelle übernehmen
settings-window-size-current-hint = Übernimmt die jetzige Fenstergröße ({ $width } × { $height })
settings-startup-note = Wirkt ab dem nächsten Start.
settings-key-hint = config.toml: { $key }
settings-note = Änderungen gelten sofort und werden in ~/.config/terminaal/config.toml gespeichert; Kommentare und Formatierung darin bleiben erhalten. Welcher Schlüssel es ist, zeigt der Tooltip am Namen der Einstellung.

## Seitenleiste: Shells, Aliase und Funktionen

shells-installed = Installierte Shells
shells-double-click = Doppelklick öffnet einen neuen Tab
shells-new-tab = ▶  Neuer Tab
shells-new-tab-hint = Öffnet einen neuen Tab mit { $shell }
shells-make-default = ★  Als Standard
shells-make-default-hint = Neue Tabs (Strg+Umschalt+T, „+“) starten mit dieser Shell
shells-managed-title = Aliase & Funktionen · { $shell }
shells-managed-unsupported = Für { $shell } kann Terminaal keine Aliase oder Funktionen verwalten – unterstützt werden fish, bash und zsh.
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

## Verwaltete Alias-/Funktionsdateien (shells/managed.rs)

managed-alias = Alias
managed-function = Funktion
managed-unsupported = diese Shell wird nicht unterstützt
managed-invalid-name = Erlaubt sind Buchstaben, Ziffern und _ . : + - (nicht am Anfang: -).
managed-header =
    # Aliase und Funktionen für Terminaal.
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
host-forward-local-hint = LocalForward: ein Port hier führt zu einem Ziel, das der Server erreicht
host-forward-remote-hint = RemoteForward: ein Port auf dem Server führt zu einem Ziel, das dieser Rechner erreicht
host-remove-forward = Weiterleitung entfernen
host-local-listen-hint = Port hier, z. B. 8080
host-local-target-hint = Ziel vom Server aus, z. B. localhost:5432
host-remote-listen-hint = Port auf dem Server, z. B. 9000
host-remote-target-hint = Ziel von hier aus, z. B. localhost:3000
host-add-forward = +  Weiterleitung
host-forwards-note = Aktiv, solange der Tab verbunden ist. Vor dem Port kann eine Adresse stehen, z. B. *:8080 für alle Netzwerkschnittstellen.
host-forward-missing-port = Bei Weiterleitung { $row } fehlt der Port.
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
adv-methods = Methoden in dieser Reihenfolge
adv-host-key = Host-Key
adv-unknown-host-keys = Unbekannte Host-Keys
adv-changed-host-key-note = Ein geänderter Host-Key bricht die Verbindung immer ab.
adv-known-hosts-file = known_hosts-Datei
adv-session = Sitzung
adv-remote-command = Befehl statt Login-Shell
adv-remote-command-hint = z. B. tmux new -A -s main
adv-set-env = Umgebungsvariablen, je Zeile NAME=Wert
adv-set-env-hint = LANG=de_DE.UTF-8
adv-send-env = Lokale Variablen mitgeben
adv-env-note = Der Server übernimmt nur, was seine AcceptEnv-Liste erlaubt. TERM=… setzt den Terminaltyp.
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
opt-forward-invalid = { $keyword } „{ $spec }“: { $reason }
opt-forward-unix = Unix-Sockets werden nicht unterstützt
opt-forward-socks = dynamische Weiterleitung (SOCKS) wird nicht unterstützt
opt-forward-syntax = erwartet wird [Adresse:]Port Ziel:Port
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
conn-any-key-closes = Beliebige Taste schließt den Tab.
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
    Neuer { $kind }-Fingerprint: { $fingerprint }
    Verbindung abgebrochen. Ist die Änderung erwartet, entferne den alten Eintrag mit
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
conn-algorithms-failed = { $keyword } für { $hop }: { $err }
conn-unknown-key-type = unbekannter Schlüsseltyp

## Portweiterleitungen (ssh/forward.rs)

forward-up = Weiterleitung { $forward }
forward-failed = Weiterleitung { $forward } fehlgeschlagen: { $err }
forward-no-address = keine Adresse gefunden
