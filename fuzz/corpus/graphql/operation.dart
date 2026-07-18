const query = r'''query Viewer($id: ID!) { viewer(id: $id) { id name } }''';
client.query(query);
