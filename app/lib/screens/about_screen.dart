import 'package:flutter/material.dart';

import '../core/backend.dart';
import '../core/notices.dart';
import '../l10n/app_localizations.dart';
import '../theme/theme.dart';
import '../theme/type.dart';
import '../widgets/network_notice.dart';
import '../widgets/notice.dart';
import '../widgets/veil_button.dart';
import '../widgets/veil_screen.dart';
import 'navigation.dart';

/// What this app is a build of, and the licence texts of everything its
/// package is built from. Those texts have to go wherever the package
/// goes; here they can also be read.
class AboutScreen extends StatelessWidget {
  const AboutScreen({
    super.key,
    required this.onAndroid,
    required this.network,
    this.bundle,
  });

  /// Whether this is the app for Android, whose package has parts no
  /// other has.
  final bool onAndroid;

  /// The network this app was built for.
  final WalletNetwork network;

  /// Where the texts are read from, if not from the app's own package.
  final AssetBundle? bundle;

  String _name(AppLocalizations text, NoticePart part) => switch (part) {
    NoticePart.veil => text.aboutPartVeil,
    NoticePart.walletLibrary => text.aboutPartWallet,
    NoticePart.core => text.aboutPartCore,
    NoticePart.transport => text.aboutPartTransport,
    NoticePart.typefaces => text.aboutPartTypefaces,
    NoticePart.android => text.aboutPartAndroid,
    NoticePart.flutter => text.aboutPartFlutter,
  };

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: text.aboutTitle,
      actions: [
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        Notice(
          veilCommit.isEmpty
              ? text.aboutBuild(veilVersion)
              : text.aboutBuildFrom(veilVersion, veilCommit),
        ),
        const SizedBox(height: 8),
        NetworkNotice(network, whereAWalletBegins: true),
        const SizedBox(height: 8),
        Notice(text.aboutSource(veilSource)),
        const SizedBox(height: 8),
        Notice(onAndroid ? text.aboutLicencesAndroid : text.aboutLicences),
        const SizedBox(height: 8),
        for (final part in NoticePart.values)
          if (onAndroid || !part.androidOnly)
            _Line(
              title: _name(text, part),
              onTap: () => open(
                context,
                LicenceTextsScreen(
                  title: _name(text, part),
                  part: part,
                  bundle: bundle,
                ),
              ),
            ),
      ],
    );
  }
}

/// The licence texts of one part of the package, each on a line that
/// names what it comes with and leads to the text.
class LicenceTextsScreen extends StatefulWidget {
  const LicenceTextsScreen({
    super.key,
    required this.title,
    required this.part,
    this.bundle,
  });

  final String title;
  final NoticePart part;
  final AssetBundle? bundle;

  @override
  State<LicenceTextsScreen> createState() => _LicenceTextsScreenState();
}

class _LicenceTextsScreenState extends State<LicenceTextsScreen> {
  List<LicenceText>? _texts;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    _read();
  }

  Future<void> _read() async {
    try {
      final texts = await licenceTextsOf(widget.part, bundle: widget.bundle);
      if (mounted) setState(() => _texts = texts);
    } catch (_) {
      if (mounted) setState(() => _failed = true);
    }
  }

  /// What a text comes with, in one line: the first name, and how many
  /// more there are.
  String _names(AppLocalizations text, LicenceText licence) {
    if (licence.names.isEmpty) return widget.title;
    final others = licence.names.length - 1;
    return others == 0
        ? licence.names.first
        : text.aboutAndMore(licence.names.first, others);
  }

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    final texts = _texts;
    return VeilScreen(
      title: widget.title,
      actions: [
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        // A package without its licence texts is not as it should be,
        // and that is said, not passed over.
        if (_failed || (texts != null && texts.isEmpty))
          Notice(text.aboutTextsMissing, warning: true)
        else if (texts != null) ...[
          Notice(text.aboutTextsCount(texts.length)),
          const SizedBox(height: 8),
          for (final licence in texts)
            _Line(
              title: _names(text, licence),
              onTap: () => open(
                context,
                LicenceTextScreen(title: widget.title, licence: licence),
              ),
            ),
        ],
      ],
    );
  }
}

/// One licence text, with everything it comes with named above it.
class LicenceTextScreen extends StatelessWidget {
  const LicenceTextScreen({
    super.key,
    required this.title,
    required this.licence,
  });

  final String title;
  final LicenceText licence;

  @override
  Widget build(BuildContext context) {
    final text = AppLocalizations.of(context)!;
    return VeilScreen(
      title: title,
      actions: [
        VeilQuietButton(
          label: text.back,
          onPressed: () => Navigator.of(context).pop(),
        ),
      ],
      children: [
        if (licence.names.isNotEmpty) ...[
          Notice(licence.names.join('\n')),
          const SizedBox(height: 12),
        ],
        // As it was written: a licence is not put into other words, and
        // not into the app's language either.
        SelectableText(licence.text, style: VeilText.secondaryIn(context)),
      ],
    );
  }
}

/// One thing to read, on a line that leads to it.
class _Line extends StatelessWidget {
  const _Line({required this.title, required this.onTap});

  final String title;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(
              minHeight: VeilSizes.buttonHeight,
            ),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 8),
                child: Text(title, style: VeilText.body),
              ),
            ),
          ),
        ),
        const Divider(),
      ],
    );
  }
}
