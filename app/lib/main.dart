import 'package:flutter/widgets.dart';

import 'app.dart';
import 'core/core_backend.dart';
import 'core/paths.dart';

void main() => runApp(VeilApp(backend: CoreBackend(AppPaths.forThisDevice())));
