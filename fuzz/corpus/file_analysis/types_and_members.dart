library sample;

import 'dart:async';
import 'src/other.dart' show Other hide Hidden;
export 'src/more.dart';

mixin Logger on Object {
  void log(String message) => print(message);
}

abstract class Base<T extends Comparable<T>> implements Comparable<Base<T>> {
  Base(this.value);
  final T value;
  T get doubled => value;
  set doubled(T next) {}
  int operator +(Base<T> other) => 1;
}

class Impl extends Base<int> with Logger {
  Impl(super.value) : assert(value > 0, 'positive');
  factory Impl.zero() => Impl(0);

  Future<void> run([int times = 1]) async {
    var total = 0;
    final futures = <Future<int>>[for (var i = 0; i < times; i++) Future.value(i)];
    for (final f in futures) {
      total += await f;
    }
    log('$total ${total > 1 ? "many" : "one"}');
  }
}

enum Color { red, green; const Color(); }

extension IntX on int {
  int twice() => this * 2;
}

typedef Callback = void Function(int);
