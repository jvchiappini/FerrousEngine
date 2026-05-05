/// Identificador universal y determinista.
/// 
/// Evita la manipulación directa de Entities subyacentes del mundo ECS
/// para garantizar que el Engine pueda cambiar internamente de librería sin afectar al usuario final.
/// 
/// Si el `NodeId` no existe, la API enmudecerá el hot-reload call en vez de causar pánicos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);
