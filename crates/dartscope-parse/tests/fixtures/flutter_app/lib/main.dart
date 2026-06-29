import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_gen/gen_l10n/app_localizations.dart';
import 'src/home_model.dart';
export 'src/public_api.dart';
part 'main.g.dart';

class HomeScreen extends StatelessWidget {
  Widget build(BuildContext context) {
    return Image.asset('assets/images/logo.png');
  }
}

Future<String> loadFixture() {
  return rootBundle.loadString('assets/config/app.json');
}

String title(BuildContext context) {
  return AppLocalizations.of(context)!.homeTitle;
}

class CounterState extends State<StatefulWidget> {
}

typedef LabelBuilder = String Function(int value);
