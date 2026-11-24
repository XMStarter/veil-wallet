import 'package:flutter/material.dart';
import 'package:qr/qr.dart';

import '../theme/colors.dart';
import '../theme/theme.dart';

/// [data] as a QR code: ink on the white of a field, with the quiet zone
/// around it that scanners need.
class QrView extends StatelessWidget {
  const QrView(this.data, {super.key});

  final String data;

  @override
  Widget build(BuildContext context) {
    final image = QrImage(QrCode(payload: QrPayload.fromString(data)));
    return SizedBox.square(
      dimension: VeilSizes.qrCode,
      child: CustomPaint(painter: _QrPainter(image), isComplex: true),
    );
  }
}

class _QrPainter extends CustomPainter {
  const _QrPainter(this.image);

  final QrImage image;

  /// The standard asks for four modules of nothing on every side.
  static const _quietZone = 4;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = VeilColors.field);
    final count = image.moduleCount;
    // Every module the same whole number of pixels, which is what
    // scanners read best; what is left over widens the quiet zone.
    final across = count + 2 * _quietZone;
    final module = (size.shortestSide / across).floorToDouble();
    if (module < 1) return;
    final origin = ((size.shortestSide - module * count) / 2).floorToDouble();
    final dark = Paint()
      ..color = VeilColors.ink
      ..isAntiAlias = false;
    for (var row = 0; row < count; row++) {
      for (var column = 0; column < count; column++) {
        if (!image.isDark(row, column)) continue;
        canvas.drawRect(
          Rect.fromLTWH(
            origin + column * module,
            origin + row * module,
            module,
            module,
          ),
          dark,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_QrPainter old) => old.image != image;
}
