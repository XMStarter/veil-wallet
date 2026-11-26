import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// The version of this app, as `pubspec.yaml` has it. A test holds the
/// two together.
const veilVersion = '0.1.0';

/// The commit this app was built from, if the build said (`check.sh`
/// does, with `--dart-define=VEIL_COMMIT=...`). Empty if it did not.
const veilCommit = String.fromEnvironment('VEIL_COMMIT');

/// Where the source is.
const veilSource = 'github.com/XMStarter/veil-wallet';

/// A licence text and the parts of the package it comes with.
@immutable
class LicenceText {
  const LicenceText(this.names, this.text);

  /// The parts that carry this text, each as its name and version.
  final List<String> names;
  final String text;
}

/// What a package is built from, by the file that holds its licence
/// texts. `native/notices.sh` writes the files into `assets/licences`.
enum NoticePart {
  veil('veil.txt'),
  walletLibrary('wallet-library.txt'),
  core('core.txt'),

  /// In the app for Android only: no other package has the program.
  transport('transport.txt', androidOnly: true),
  typefaces('typefaces.txt'),
  android('android.txt', androidOnly: true),

  /// Flutter, its engine and the Dart packages. Their texts are not in a
  /// file of ours: Flutter puts them into the app when it is built.
  flutter(null);

  const NoticePart(this.file, {this.androidOnly = false});

  final String? file;
  final bool androidOnly;
}

/// What divides one licence text from the next in the files: a line of
/// eighty dashes, as `veil_notices` writes it.
final _divider = '\n${'-' * 80}\n';

/// Reads a file of licence texts as `veil_notices` writes them: entries
/// divided by a line of eighty dashes, each the names of the parts it is
/// about, one to a line, then an empty line, then the text. An entry that
/// is not of that shape is kept whole, without names, so that no text is
/// lost to a file that was written some other way.
List<LicenceText> licenceTextsIn(String file) {
  final texts = <LicenceText>[];
  for (final entry in file.replaceAll('\r\n', '\n').split(_divider)) {
    final whole = entry.trim();
    if (whole.isEmpty) continue;
    final gap = whole.indexOf('\n\n');
    if (gap < 0) {
      texts.add(LicenceText(const [], whole));
      continue;
    }
    final names = [
      for (final line in whole.substring(0, gap).split('\n'))
        if (line.trim().isNotEmpty) line.trim(),
    ];
    texts.add(LicenceText(names, whole.substring(gap + 2).trim()));
  }
  return texts;
}

/// The licence texts of [part], from the app's own package. Nothing is
/// fetched from anywhere.
Future<List<LicenceText>> licenceTextsOf(
  NoticePart part, {
  AssetBundle? bundle,
}) async {
  final file = part.file;
  if (file != null) {
    final text = await (bundle ?? rootBundle).loadString(
      'assets/licences/$file',
      // Read here, in one piece: these are texts of up to a megabyte.
      cache: false,
    );
    return licenceTextsIn(text);
  }
  final texts = <LicenceText>[];
  await for (final entry in LicenseRegistry.licenses) {
    texts.add(
      LicenceText(
        entry.packages.toList()..sort(),
        entry.paragraphs.map((paragraph) => paragraph.text).join('\n\n'),
      ),
    );
  }
  texts.sort((a, b) => a.names.join().compareTo(b.names.join()));
  return texts;
}
