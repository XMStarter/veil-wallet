import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veil/theme/colors.dart';
import 'package:veil/theme/theme.dart';
import 'package:veil/widgets/veil_button.dart';

import 'support.dart';

void main() {
  setUpAll(loadFonts);

  testWidgets('the start screen offers the two ways to a wallet, in English', (
    tester,
  ) async {
    await pumpApp(tester, 'en');
    expect(find.text('CREATE WALLET'), findsOneWidget);
    expect(find.text('RESTORE FROM RECOVERY PHRASE'), findsOneWidget);
    expect(
      find.text('A Monero wallet. Everything goes through Tor.'),
      findsOneWidget,
    );
    expect(
      find.textContaining('does not work with real funds'),
      findsOneWidget,
    );
    expect(find.bySemanticsLabel('Veil'), findsOneWidget);
  });

  testWidgets('and in Russian', (tester) async {
    await pumpApp(tester, 'ru');
    expect(find.text('СОЗДАТЬ КОШЕЛЁК'), findsOneWidget);
    expect(find.text('ВОССТАНОВИТЬ ПО ФРАЗЕ'), findsOneWidget);
    expect(find.text('Кошелёк Monero. Всё идёт через Tor.'), findsOneWidget);
    expect(
      find.textContaining('не работает с настоящими средствами'),
      findsOneWidget,
    );
  });

  testWidgets('a language the app does not have falls back to English', (
    tester,
  ) async {
    await pumpApp(tester, 'fr');
    expect(find.text('CREATE WALLET'), findsOneWidget);
  });

  for (final language in ['en', 'ru']) {
    testWidgets(
      'buttons are 54 px high, in one column, with one primary ($language)',
      (tester) async {
        await pumpApp(tester, language);
        final buttons = find.byType(VeilButton);
        expect(buttons, findsNWidgets(2));
        for (final button in buttons.evaluate()) {
          final size = tester.getSize(find.byWidget(button.widget));
          expect(size.height, VeilSizes.buttonHeight);
          expect(size.width, windowSize.width - 2 * VeilSizes.screenPadding);
        }
        // One main action per screen.
        expect(find.byType(FilledButton), findsOneWidget);
        expect(find.byType(OutlinedButton), findsOneWidget);
        final filled = tester.widget<FilledButton>(find.byType(FilledButton));
        final style =
            filled.style ??
            Theme.of(tester.element(find.byType(FilledButton)))
                .filledButtonTheme
                .style!;
        expect(style.backgroundColor!.resolve({}), VeilColors.ink);
        final shape = style.shape!.resolve({})! as RoundedRectangleBorder;
        expect(shape.borderRadius, const BorderRadius.all(Radius.circular(2)));
        // Nothing is cut off or spills over in the narrow window.
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'in a wide desktop window the column stays as narrow as a phone',
    (tester) async {
      tester.view.physicalSize = const Size(1600, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await pumpAppWide(tester);
      final button = tester.getSize(find.byType(VeilButton).first);
      expect(button.width, VeilSizes.columnWidth - 2 * VeilSizes.screenPadding);
    },
  );

  testWidgets('each way leads to its screen and back', (tester) async {
    await pumpApp(tester, 'en');
    await tester.tap(find.text('CREATE WALLET'));
    await tester.pumpAndSettle();
    expect(find.text('NEW WALLET'), findsOneWidget);
    expect(find.text('This screen is not built yet.'), findsOneWidget);
    await tester.tap(find.text('BACK'));
    await tester.pumpAndSettle();
    expect(find.text('CREATE WALLET'), findsOneWidget);

    await tester.tap(find.text('RESTORE FROM RECOVERY PHRASE'));
    await tester.pumpAndSettle();
    expect(find.text('RESTORE'), findsOneWidget);
  });
}
