import 'package:flutter/material.dart';

class CounterPage extends StatefulWidget {
  const CounterPage({super.key});

  @override
  State<CounterPage> createState() => _CounterPageState();
}

class _CounterPageState extends State<CounterPage> {
  int _count = 0;

  void _increment() {
    setState(() {
      _count++;
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text('Count: $_count')),
      body: Column(
        children: [
          for (var i = 0; i < _count; i++) Text('row $i', style: theme.textTheme.bodyMedium),
          ElevatedButton(onPressed: _increment, child: const Text('Add')),
        ],
      ),
    );
  }
}
