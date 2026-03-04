//! High-level typed query types built on top of `World::query*`.
//!
//! `Query<C>` provides a cached, archetype-indexed view into the world that
//! can be iterated repeatedly without re-scanning archetypes.  For most use
//! cases the `World::query` / `World::query2` / `World::query3` methods are
//! sufficient, but `Query` is useful when you want to store the query result
//! across multiple system invocations.

use crate::component::Component;
use crate::entity::Entity;
use crate::world::World;

// ---------------------------------------------------------------------------
// Immutable Query

/// A typed, re-usable single-component query.
///
/// Call `Query::new(&world)` to build the query (scans archetypes once), then
/// iterate with `query.iter()`.  The query becomes invalid if the world
/// structure changes (`world.change_tick` advances); rebuild it in that case.
pub struct Query<'w, C: Component> {
    world: &'w World,
    /// Indices of archetypes that contain `C`.
    matching: Vec<usize>,
    _marker: std::marker::PhantomData<&'w C>,
}

impl<'w, C: Component> Query<'w, C> {
    /// Scan `world` for archetypes containing `C`.
    pub fn new(world: &'w World) -> Self {
        let matching: Vec<usize> = world
            .archetypes
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| a.column::<C>().is_some())
            .map(|(i, _)| i)
            .collect();
        Query {
            world,
            matching,
            _marker: std::marker::PhantomData,
        }
    }

    /// Iterate over `(Entity, &C)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &C)> + '_ {
        self.matching.iter().flat_map(move |&arch_id| {
            let arch = &self.world.archetypes.archetypes[arch_id];
            let col = arch.column::<C>().unwrap();
            arch.entities.iter().enumerate().map(move |(row, &entity)| {
                let comp = unsafe { col.get::<C>(row) };
                (entity, comp)
            })
        })
    }

    /// Number of matching entities.
    pub fn len(&self) -> usize {
        self.matching
            .iter()
            .map(|&id| self.world.archetypes.archetypes[id].len())
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Mutable Query

/// A typed, single-component mutable query.
pub struct QueryMut<'w, C: Component> {
    world: &'w mut World,
    matching: Vec<usize>,
    _marker: std::marker::PhantomData<&'w mut C>,
}

impl<'w, C: Component> QueryMut<'w, C> {
    pub fn new(world: &'w mut World) -> Self {
        let matching: Vec<usize> = world
            .archetypes
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| a.column::<C>().is_some())
            .map(|(i, _)| i)
            .collect();
        QueryMut {
            world,
            matching,
            _marker: std::marker::PhantomData,
        }
    }

    /// Iterate with `(Entity, &mut C)` pairs.
    ///
    /// Note: This requires an exclusive borrow of the world for the duration
    /// of iteration.  For concurrent access patterns, prefer splitting systems.
    pub fn for_each_mut<F: FnMut(Entity, &mut C)>(&mut self, mut f: F) {
        for &arch_id in &self.matching {
            let count = self.world.archetypes.archetypes[arch_id].entities.len();
            for row in 0..count {
                // SAFETY: row < count, arch_id is valid, C is the correct type
                let entity = self.world.archetypes.archetypes[arch_id].entities[row];
                let arch = &mut self.world.archetypes.archetypes[arch_id];
                let col = arch.column_mut::<C>().unwrap();
                let comp = unsafe { col.get_mut::<C>(row) };
                f(entity, comp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::world::World;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(f32);
    impl Component for Pos {}

    #[derive(Clone, Debug, PartialEq)]
    struct Vel(f32);
    impl Component for Vel {}

    #[test]
    fn query_finds_matching() {
        let mut world = World::new();
        world.spawn((Pos(1.0),));
        world.spawn((Pos(2.0), Vel(1.0)));
        world.spawn((Vel(3.0),));

        let q = Query::<Pos>::new(&world);
        assert_eq!(q.len(), 2);

        let positions: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        assert!(positions.contains(&1.0));
        assert!(positions.contains(&2.0));
    }

    #[test]
    fn query_mut_updates() {
        let mut world = World::new();
        world.spawn((Pos(0.0),));
        world.spawn((Pos(5.0),));

        let mut qm = QueryMut::<Pos>::new(&mut world);
        qm.for_each_mut(|_, p| p.0 += 1.0);

        let q = Query::<Pos>::new(&world);
        let mut vals: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(vals, vec![1.0, 6.0]);
    }
}
