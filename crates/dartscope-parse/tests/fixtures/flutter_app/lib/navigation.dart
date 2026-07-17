import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

// Named route constants resolved by DartScope.
const String homeRoute = '/';
const String settingsRoute = '/settings';
const String profileRoute = '/profile/:id';

/// GoRouter configuration using route constants.
final GoRouter appRouter = GoRouter(
  routes: [
    GoRoute(
      path: homeRoute,
      name: 'home',
      builder: (context, state) => const HomeScreen(),
    ),
    GoRoute(
      path: settingsRoute,
      name: 'settings',
      builder: (context, state) => const SettingsScreen(),
    ),
    GoRoute(
      path: profileRoute,
      builder: (context, state) => ProfileScreen(id: state.pathParameters['id']!),
    ),
  ],
);

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Home')),
      body: Column(
        children: [
          ElevatedButton(
            onPressed: () => context.go(settingsRoute),
            child: const Text('Settings'),
          ),
          // Official named Navigator navigation is derived by dartscope-flutter.
          ElevatedButton(
            onPressed: () => Navigator.pushNamed(context, '/legacy'),
            child: const Text('Legacy'),
          ),
        ],
      ),
    );
  }
}

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: ElevatedButton(
        onPressed: () => context.pop(),
        child: const Text('Back'),
      ),
    );
  }
}

class ProfileScreen extends StatelessWidget {
  final String id;

  const ProfileScreen({super.key, required this.id});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Text('Profile: $id'),
    );
  }
}
