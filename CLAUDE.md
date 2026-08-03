# CLAUDE.md

Zasady pracy nad tym projektem. Czytaj przed każdą sesją i trzymaj się ich bez przypominania.

---

## Czym jest ten projekt

Desktopowa aplikacja do zgrywania płyt CD Audio. Cel: jedno narzędzie, które w jednym przebiegu daje bit-perfect rip z weryfikacją, komplet metadanych, okładkę, gatunek, poprawne numery płyt w wydaniach wielopłytowych i teksty piosenek.

Użytkownik wybiera format wyjściowy: **FLAC, WAV, AIFF, ALAC, APE, MP3, AAC, Ogg Vorbis, M4A**. Można wybrać kilka naraz — płyta jest czytana raz, a enkodowanie do wszystkich wybranych formatów leci równolegle z ripu.

## Platformy

**Windows, Linux i macOS są równorzędnymi celami od pierwszego commita.** Nie ma platformy „głównej" i nie odkłada się przenośności na później — dopisywanie wsparcia dla innych systemów po fakcie oznaczałoby przepisanie warstwy dostępu do napędu i całego buildu.

Konsekwencje, które obowiązują cały czas:

- Kod specyficzny dla systemu żyje wyłącznie w module `drive`, za wspólnym traitem. Nigdzie indziej nie ma `#[cfg(target_os)]`.
- Żadnych zahardkodowanych ścieżek. Katalogi konfiguracji, cache i muzyki bierz z API ścieżek Tauri albo z crate'a `directories`.
- Każda nowa zależność musi budować się na wszystkich trzech systemach. Jeśli nie — nie wchodzi.
- `docs/BUILD.md` opisuje kompletne, powtarzalne kroki dla **każdej** platformy osobno.
- Konfiguracja CI (GitHub Actions, matrix na trzech systemach) jest w repo od początku. Commituj ją normalnie — po prostu nigdy nie pushuj.

---

## Licencja: GPL-3.0-or-later

Nie jest to wybór estetyczny. Linkujemy `libcdio-paranoia`, które jest na GPL, więc cała aplikacja musi być GPL. Świadomie odrzuciliśmy architekturę sidecar właśnie po to, żeby móc linkować bezpośrednio.

- Każdy plik `.rs` zaczyna się od `// SPDX-License-Identifier: GPL-3.0-or-later`
- Przed dodaniem jakiejkolwiek zależności sprawdź jej licencję i dopisz ją do tabeli w README
- Nie proponuj zmiany licencji na luźniejszą — to nie jest możliwe przy tej architekturze

---

## Git

- Commituj często, małymi logicznymi krokami. Każdy commit zostawia repo w stanie, który się kompiluje.
- **Nigdy `git push`.** Ani razu, pod żadnym pretekstem. Tylko lokalne commity.
- Nie twórz PR-ów, nie dodawaj remote'ów, nie używaj `gh`.
- Nie commituj bez uruchomienia `cargo fmt`, `cargo clippy -- -D warnings` i lintera frontu.

### Wiadomości commitów

Conventional Commits, tryb rozkazujący, po angielsku, temat do 72 znaków. Body tylko wtedy, gdy wyjaśnia *dlaczego*, nie *co*.

```
feat(toc): read raw TOC via IOCTL_CDROM_READ_TOC_EX
fix(paranoia): handle READERR status without aborting rip
refactor(metadata): extract MusicBrainz client into own module
test(discid): add known-TOC fixtures for multi-session discs
```

**Zabronione w commitach i w kodzie:**

- trailery `Co-Authored-By:` — jakiekolwiek
- stopki typu „Generated with…", linki do narzędzi, wzmianki o AI
- emoji
- zwroty w pierwszej osobie („I've added…")

Komentarze w kodzie pisz jak człowiek: rzadko, tylko tam gdzie kod jest nieoczywisty, i wyjaśniaj powód, a nie treść linijki. Żadnych komentarzy opisujących oczywistości ani nagłówków sekcji w stylu wygenerowanego kodu.

---

## Zależności

### pnpm, wyłącznie

Nigdy npm ani yarn. Nie commituj `package-lock.json` ani `yarn.lock`.

### Nie zgaduj wersji z pamięci

Zanim dodasz cokolwiek, sprawdź aktualną wersję:

```bash
pnpm view <pakiet> version      # albo od razu: pnpm add <pakiet>@latest
cargo add <crate>               # samo rozwiąże najnowszą
```

Nie wpisuj wersji ręcznie do `Cargo.toml` ani `package.json`. Po instalacji sprawdź konflikty peer dependencies i czy build przechodzi. Przy większych skokach wersji przejrzyj changelog pod kątem breaking changes.

Preferuj mniej zależności. Jeśli coś da się napisać w 30 liniach zamiast dociągać crate — pisz sam.

---

## Skille

Przed pracą nad daną częścią kodu sprawdź dostępne skille i użyj tych, które pasują — nie zakładaj, że wiesz, jak coś zrobić w tym środowisku. W szczególności przed pisaniem lub przeprojektowywaniem UI (komponenty, style, layout, typografia) sięgnij po skill od frontend designu. Przy zadaniach z innej domeny czytaj odpowiedni skill zanim napiszesz pierwszą linijkę.

---

## Weryfikacja API

Nie wymyślaj sygnatur funkcji, nazw pól JSON ani endpointów. Przy braku pewności sprawdź dokumentację — docs.rs, oficjalne API docs, źródła crate'a. Dotyczy to szczególnie: libcdio, MusicBrainz WS/2, Cover Art Archive, CTDB, LRCLIB i API Tauri 2.

---

## Struktura

```
src-tauri/          warstwa Tauri: komendy, stan aplikacji, bundling
crates/cdio-sys/    bindingi FFI do libcdio / libcdio-paranoia
crates/core/        TOC, discid, rip, weryfikacja, metadane, tagowanie
app/                Nuxt
docs/BUILD.md       powtarzalne kroki buildu na czystej maszynie
```

Rdzeń logiki żyje w `crates/core` i nie zna Tauri. Komendy Tauri są cienką warstwą nad nim — dzięki temu rip i metadane da się testować bez odpalania GUI.

### Moduły `core`

| Moduł | Odpowiedzialność |
| --- | --- |
| `drive` | wykrywanie napędów, TOC, stan tacki, wysuwanie — trait `Drive` z implementacjami per system, `libcdio` jako wspólna baza |
| `toc` | surowy TOC: offsety ścieżek, lead-out, pre-emphasis, flagi data track |
| `discid` | MusicBrainz Disc ID liczony samodzielnie z TOC + FreeDB ID |
| `rip` | ekstrakcja przez paranoia, offset napędu, pregapy, retry, statusy paranoi |
| `encode` | trait `Encoder` + rejestr formatów, enkodowanie równoległe do wielu formatów z jednego odczytu |
| `verify` | CRC32 w stylu EAC, sumy AccurateRip V1/V2 lokalnie, sprawdzenie online w CTDB |
| `metadata` | kaskada źródeł identyfikacji płyty, ręczne wyszukiwanie jako ostatnia deska ratunku |
| `lyrics` | LRCLIB, dopasowanie po czasie trwania |
| `tag` | zapis przez `lofty`, okładka, CUE sheet, log ripu |

---

## Rzeczy, na których łatwo się przewrócić

**Disc ID.** Błąd o jeden sektor psuje wszystko dalej i jest bardzo trudny do wyśledzenia, bo objawia się dopiero jako „nie znaleziono wydania". Testy jednostkowe na zahardkodowanych, znanych TOC-ach są obowiązkowe.

**Wiele wydań pod jednym Disc ID.** To normalna sytuacja, nie błąd. Nigdy nie wybieraj automatycznie — wystaw listę do UI i pozwól użytkownikowi zdecydować.

**Offset napędu** jest w samplach, konwencja jak w EAC. Zły offset daje rip, który wygląda poprawnie, ale nie zgadza się z żadną bazą weryfikacji.

**Baza AccurateRip** należy do Illustrate i wymaga zgody na użycie w cudzym oprogramowaniu. Sam algorytm jest jawny. Sumy licz i pokazuj lokalnie, ale **nie odpytuj ich bazy** — do weryfikacji online używaj CTDB.

**Gatunek** w MusicBrainz jest ubogi. Discogs lub Last.fm jako drugie źródło, token w ustawieniach. Brak tokenu nie może wywracać aplikacji — po prostu nie ma gatunku.

**Teksty.** `duration` do LRCLIB bierz z TOC, w sekundach. Synced zapisuj dodatkowo jako `.lrc` obok utworu — większość odtwarzaczy tak to woli.

**M4A to kontener, nie kodek.** W UI muszą być dwie osobne pozycje: `M4A (AAC)` i `M4A (ALAC)`. Samo „m4a" jest niejednoznaczne i użytkownik dostanie coś innego, niż myślał.

**APE nie ma enkodera w FFmpeg** — tylko dekoder, w dodatku ograniczony. Enkodowanie wymaga oficjalnego Monkey's Audio SDK, dołączanego jako osobna, opcjonalna zależność za feature flagą `ape`. Bez tej flagi aplikacja się buduje i działa, tylko bez APE.

**AAC: używaj natywnego enkodera FFmpeg.** Nigdy `fdk-aac` — wymaga `--enable-nonfree`, co czyni binarkę nieredystrybuowalną i jest nie do pogodzenia z GPL.

**Ogg Vorbis: buduj FFmpeg z `--enable-libvorbis`.** Natywny enkoder Vorbisa w FFmpeg jest wyraźnie gorszy jakościowo.

**macOS montuje płyty audio automatycznie.** System podpina CD jako wolumin z plikami `.aiff` i blokuje surowy odczyt urządzenia. Przed ripem trzeba odmontować dysk (`diskutil unmountDisk`), a po zakończeniu przywrócić stan. To najczęstsza przyczyna „nie mogę otworzyć urządzenia" na macu. Dodatkowo: Apple Silicon nie ma wbudowanych napędów, więc testuj wyłącznie na zewnętrznym USB.

**Linux: uprawnienia do urządzenia.** `/dev/sr0` zwykle wymaga przynależności do grupy `cdrom` albo `optical`. Jeśli otwarcie urządzenia się nie powiedzie, komunikat błędu ma o tym mówić wprost i podawać nazwę grupy — nie „odmowa dostępu".

**Sanityzacja nazw plików różni się między systemami.** Linux i macOS dopuszczają `:` i inne znaki zakazane na Windows. Domyślnie stosuj wariant najbardziej restrykcyjny (windowsowy) niezależnie od platformy — biblioteka muzyczna często ląduje na NAS-ie albo dysku współdzielonym z Windows. Luźniejszy tryb dopuść tylko jako świadomy wybór w ustawieniach.

**Offset napędu jest cechą modelu napędu, nie systemu.** Zapisuj go per urządzenie, po identyfikatorze producent/model, żeby ten sam napęd nie wymagał ponownej kalibracji po przełączeniu systemu.

---

## Źródła metadanych

Nie opieramy się na jednym źródle. Płyta ma zostać rozpoznana kaskadą prób, od najbardziej wiarygodnej do najluźniejszej. Kolejne kroki uruchamiają się dopiero wtedy, gdy poprzedni nic nie zwrócił — ale wynik z każdego źródła musi być oznaczony, skąd pochodzi, żeby użytkownik wiedział, czemu ufa.

1. **CD-TEXT z samej płyty** — dane zapisane na dysku, bez internetu. Rzadkie, ale gdy są, są autorytatywne. Czytaj przez libcdio.
2. **MusicBrainz po Disc ID** — główne źródło. `inc=recordings+artist-credits+release-groups+labels`.
3. **CTDB (CUETools DB)** — pełni podwójną rolę: weryfikacja ripu i metadane. Replikuje MusicBrainz, Discogs i freeDB, i obsługuje wyszukiwanie rozmyte po CDTOC, więc trafia tam, gdzie sam Disc ID nie trafił.
4. **GnuDB** — następca freedb (`gnudb.gnudb.org`, protokół CDDB1, port 8880 lub HTTP na `/~cddb/cddb.cgi`). Dane bywają brudne, ale przy starszych i niszowych wydaniach czasem jako jedyne coś mają.
5. **MCN/UPC ze subkanału płyty → Discogs po barcode.** To jest niedoceniana ścieżka: fizyczna płyta często niesie kod kreskowy w Media Catalogue Number, a Discogs ma najlepszą bazę wydań fizycznych. Odczyt MCN przez libdiscid lub libcdio.
6. **ISRC per ścieżka → MusicBrainz po ISRC.** Ratuje sytuację, gdy wydanie jest nieznane, ale nagrania już tak.
7. **Ręczne wyszukiwanie w aplikacji** — patrz niżej. Zawsze dostępne, nie tylko po niepowodzeniu automatu.

Uzupełniająco, niezależnie od tego, które źródło trafiło:

- **Okładka:** Cover Art Archive → obrazy z Discogs → iTunes Search API (bez klucza) → wgranie własnego pliku przez użytkownika.
- **Gatunek:** MusicBrainz jest tu ubogi. Dołóż Discogs (`genre` + `style`) i Last.fm (tagi). Brak tokenu któregokolwiek nie może wywracać aplikacji — po prostu nie ma tego źródła.

Zasady dla całej warstwy:

- Każde źródło za wspólnym traitem `MetadataSource`, rejestrowane w kaskadzie. Dodanie kolejnej bazy ma być jedną implementacją.
- Awaria lub timeout jednego źródła **nigdy** nie przerywa całości — leć dalej, odnotuj w logu.
- Wyniki z kilku źródeł pokaż użytkownikowi obok siebie z etykietą pochodzenia. Nie scalaj ich po cichu i nie wybieraj automatycznie, gdy się różnią.
- Klucze i tokeny API trzymaj w ustawieniach, nigdy w repo. Aplikacja ma być w pełni użyteczna bez żadnego z nich.
- Respektuj limity zapytań i wymagany User-Agent każdego serwisu.

### Ręczne wyszukiwanie

Gdy żadne źródło nic nie zwróci — albo gdy zwróci coś błędnego — użytkownik musi mieć możliwość samodzielnego znalezienia płyty. To nie jest funkcja awaryjna doklejona na końcu, tylko normalna ścieżka.

- Pole wyszukiwania po tytule i wykonawcy, przeszukujące MusicBrainz i Discogs jednocześnie.
- Wyniki z liczbą ścieżek i czasem trwania, żeby dało się je porównać z TOC płyty w napędzie. Podświetl te, gdzie liczba ścieżek się zgadza.
- Możliwość wklejenia bezpośredniego linku lub identyfikatora wydania z MusicBrainz albo Discogs.
- Pełna ręczna edycja wszystkich pól, łącznie z tytułami poszczególnych ścieżek — jako ostatnia deska ratunku dla płyt, których nie ma nigdzie (składanki, wydania lokalne, dema).
- Ręcznie wprowadzone dane zapisuj lokalnie i przypisuj do Disc ID, żeby ta sama płyta włożona ponownie została rozpoznana od razu.
- Zaproponuj wysłanie uzupełnionych danych do MusicBrainz — ale **wyłącznie** po wyraźnym potwierdzeniu przez użytkownika, nigdy automatycznie.

---

## Konwencje kodu

- Rust: `cargo fmt` i `cargo clippy -- -D warnings` przechodzą przed każdym commitem
- Błędy: `thiserror` w warstwie bibliotecznej, `anyhow` na granicy komend Tauri
- Logowanie: `tracing`
- `unsafe` tylko w `cdio-sys` i module `drive`. Nigdzie indziej.
- Frontend: `<script setup lang="ts">`, Composition API, ESLint + Prettier
- **CSS: wyłącznie Tailwind.** Żadnych `<style scoped>`, plików `.css` per komponent ani bibliotek komponentów niosących własne style. Wartości arbitralne (`w-[327px]`) tylko wtedy, gdy skala Tailwinda naprawdę nie ma odpowiednika — nie jako pierwszy odruch. Powtarzające się układy klas wydzielaj do komponentów, nie do `@apply`.
- Instalacja Tailwinda: sprawdź aktualną wersję i **oficjalną** metodę integracji z Nuxtem (od v4 jest to plugin Vite `@tailwindcss/vite`, konfiguracja motywu przez `@theme` w CSS zamiast `tailwind.config.js`). Nie sugeruj się poradnikami do v3 — setup zmienił się zasadniczo.
- Nazwy w kodzie po angielsku — zawsze, bez wyjątków.

---

## Wielojęzyczność

Aplikacja jest wielojęzyczna od pierwszego commita. Nie ma etapu „najpierw po polsku, i18n dorobimy później" — dokładanie tłumaczeń do gotowego UI oznacza przeczesywanie każdego komponentu ręcznie.

- **Angielski jest językiem źródłowym**, polski pierwszym tłumaczeniem. Klucze i wartości domyślne pisz po angielsku — projekt jest open source i tłumaczenia będą przychodzić od ludzi, którzy polskiego nie znają.
- Dodanie kolejnego języka ma polegać na wrzuceniu jednego pliku do katalogu z tłumaczeniami. Zero zmian w kodzie.
- **Żadnych stringów wklejonych do komponentów.** Każdy tekst widoczny dla użytkownika przechodzi przez i18n — łącznie z etykietami przycisków, tooltipami, placeholderami, tytułami okien i komunikatami błędów.
- Do Nuxta użyj oficjalnego modułu i18n. Sprawdź aktualną wersję i metodę konfiguracji, nie sugeruj się starszymi poradnikami.
- Język wykrywaj z ustawień systemu przez API Tauri, z możliwością ręcznej zmiany w ustawieniach. Fallback na angielski, gdy tłumaczenia brakuje — nigdy goły klucz na ekranie.
- Klucze buduj hierarchicznie i opisowo: `rip.progress.verifying`, `metadata.source.musicbrainz`, `error.drive.permissionDenied`. Nie `label1`, nie pełne zdania jako klucze.
- Liczby, daty i czasy trwania formatuj przez `Intl`, nie ręcznie. Separator dziesiętny i format daty różnią się między lokalizacjami.
- Teksty z liczebnikami zawsze przez formy mnogie i18n — polski ma trzy formy (`1 ścieżka`, `2 ścieżki`, `5 ścieżek`), a angielski dwie. Sklejanie liczby ze stringiem to błąd.

**Warstwa Rust nie tłumaczy niczego.** Backend zwraca kody błędów i parametry, nigdy gotowe zdania dla użytkownika:

```rust
DriveError::PermissionDenied { device: "/dev/sr0", group: "cdrom" }
```

Front mapuje to na `error.drive.permissionDenied` i podstawia parametry. Jeśli backend zacznie zwracać teksty do wyświetlenia, cała warstwa tłumaczeń przestaje działać — a wykryjesz to dopiero przy drugim języku.

**Czego nie tłumaczymy:** nazw formatów i kodeków, tagów, nazw pól metadanych, treści z baz zewnętrznych, szablonów nazw plików ani logów technicznych. Log ripu jest artefaktem diagnostycznym i zostaje po angielsku.

---

## Pozostałe konwencje

- Progress ripu leci przez `tauri::ipc::Channel<T>`, nie przez globalne eventy. Typy zdarzeń definiuj raz i generuj z nich TS.
- Komunikaty błędów dla użytkownika mają być konkretne. „Nie udało się odczytać płyty" jest bezużyteczne — napisz, która ścieżka, na którym sektorze i co zwrócił napęd.

### Testy obowiązkowe

Disc ID, parsowanie TOC, CRC AccurateRip, szablony nazw plików, mapowanie format → kodek + kontener. Reszta wedle uznania.

---

## Enkodowanie

Warstwa enkodowania stoi na **FFmpeg** (`ffmpeg-next` albo `rsmpeg` — wybierz jeden i uzasadnij wybór w commicie). Jedna zależność zamiast ośmiu osobnych bibliotek, a licencyjnie build LGPL jest w porządku.

| Format | Kodek | Kontener | Uwagi |
| --- | --- | --- | --- |
| FLAC | flac | FLAC | domyślny, maksymalna kompresja |
| WAV | PCM s16le | WAV | |
| AIFF | PCM s16be | AIFF | |
| ALAC | alac | M4A | |
| M4A (AAC) | aac (natywny FFmpeg) | MP4 | VBR, bitrate konfigurowalny |
| MP3 | libmp3lame | MP3 | VBR, bitrate konfigurowalny |
| AAC | aac (natywny FFmpeg) | ADTS | |
| Ogg Vorbis | libvorbis | Ogg | wymaga `--enable-libvorbis` |
| APE | Monkey's Audio SDK | APE | za feature flagą `ape`, patrz niżej |

Zasady:

- **Płyta czytana jest raz.** Enkodowanie do wszystkich wybranych formatów leci równolegle z odczytu, nie sekwencyjnie po nim. Powtórny odczyt tej samej płyty to błąd projektowy.
- Każdy format za traitem `Encoder`. Dodanie kolejnego ma być kwestią jednej implementacji, bez dotykania modułu `rip`.
- Ustawienia jakości per format (bitrate lub VBR quality) trzymaj w konfiguracji, nie hardkoduj.
- Tagi i okładka mają trafić do **każdego** wybranego formatu — `lofty` ogarnia Vorbis comments, ID3, atomy MP4 i tagi APE. Okładka w Ogg Vorbis idzie jako `METADATA_BLOCK_PICTURE`.
- APE: enkoder istnieje wyłącznie w oficjalnym Monkey's Audio SDK. Przed dołączeniem przeczytaj jego aktualny tekst licencji i potwierdź zgodność z GPL-3.0 — historycznie miał nietypowe warunki. Trzymaj to za opcjonalną feature flagą, żeby brak SDK nie blokował buildu.

---

## Czego nie robić

- Nie używaj sidecara ani wywołań zewnętrznych binarek do ripowania. Cała logika w naszym procesie.
- Nie buduj FFmpeg z `--enable-nonfree` ani z `fdk-aac`. Wystarczy build LGPL z `libmp3lame` i `libvorbis`.
- Nie pisz własnych enkoderów dla formatów, które FFmpeg już obsługuje.
- Nie odpytuj bazy AccurateRip.
- Nie rozlewaj `unsafe` poza dwa wyznaczone miejsca.
- Nie wklejaj tekstów widocznych dla użytkownika do komponentów ani do kodu Rusta — wszystko przez i18n.
- Nie zwracaj z backendu gotowych zdań dla użytkownika. Kody błędów i parametry, tłumaczenie po stronie frontu.
- Nie wstawiaj `#[cfg(target_os)]` poza modułem `drive`. Jeśli ci się to gdzieś zaczyna zdawać potrzebne, to znak, że abstrakcja jest w złym miejscu.
- Nie hardkoduj ścieżek ani separatorów. Nigdzie.
- Nie dodawaj zależności, która nie buduje się na wszystkich trzech systemach.
- Nie pushuj.
