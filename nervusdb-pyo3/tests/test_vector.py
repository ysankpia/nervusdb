#!/usr/bin/env python3
"""Vector search integration test for nervusdb Python bindings."""

import nervusdb
import tempfile
import os
import math

def euclidean_distance(v1, v2):
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(v1, v2)))

def test_vector_search():
    """Test vector insert and search."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = os.path.join(tmpdir, "vector.ndb")
        
        # 1. Open Database
        db = nervusdb.open(db_path)
        print(f"✓ Opened database at {db.path}")
        
        # 2. Insert Nodes with Vectors
        txn = db.begin_write()
        
        # Node 1: Origin
        n1 = txn.create_node("vec_1", "Point")
        txn.set_vector(n1, [0.0, 0.0])
        
        # Node 2: On X-axis
        n2 = txn.create_node("vec_2", "Point")
        txn.set_vector(n2, [1.0, 0.0])
        
        # Node 3: On Y-axis
        n3 = txn.create_node("vec_3", "Point")
        txn.set_vector(n3, [0.0, 1.0])
        
        # Node 4: Far away
        n4 = txn.create_node("vec_4", "Point")
        txn.set_vector(n4, [100.0, 100.0])
        
        txn.commit()
        print("✓ Created 4 nodes with vectors")
        
        # 3. Search closest to [0.1, 0.1] (Should be Node 1: [0,0])
        query = [0.1, 0.1]
        results = db.search_vector(query, 2)
        print(f"✓ Search query {query}: {results}")
        
        assert len(results) == 2
        
        # Check Node 1 is first result
        top_id, top_dist = results[0]
        # n1 should be the closest. Note: InternalNodeId starts at 1 usually, but let's just check relative order or distance.
        # But we know n1 was created first.
        
        # Let's verify distances
        # Dist to [0,0] is sqrt(0.1^2 + 0.1^2) = sqrt(0.02) = ~0.1414
        expected_dist = euclidean_distance(query, [0.0, 0.0])
        assert abs(top_dist - expected_dist) < 1e-4, f"Expected {expected_dist}, got {top_dist}"
        
        # 4. Search closest to [50, 50] (Should be Node 4)
        query2 = [50.0, 50.0]
        results2 = db.search_vector(query2, 1)
        print(f"✓ Search query {query2}: {results2}")
        assert results2[0][0] == n4, "Top result should be Node 4"
        
        # 5. Persistence Test (Reopen)
        db.close()
        print("✓ Closed database")
        
        db = nervusdb.open(db_path)
        print("✓ Reopened database")
        
        results3 = db.search_vector(query, 1)
        print(f"✓ Re-Search query {query}: {results3}")
        assert results3[0][0] == n1, "Persistence check: Node 1 should still be closest"
        
    print("\n🎉 Vector search tests passed!")

if __name__ == "__main__":
    test_vector_search()
