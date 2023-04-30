use std::cmp;
use std::fmt;
use std::borrow::Borrow;
use std::collections::HashSet;


#[derive(Clone, Default)]
pub struct Graph<N: Clone, E: Clone = ()> {
    nodes: Vec<N>,
    edges: Vec<Vec<E>>,
    graph: Vec<Vec<usize>>,
}

impl<N: Eq + Clone, E: Default + Clone> Graph<N, E> {
    pub fn new() -> Self {
        Self { graph: Vec::new(), nodes: Vec::new(), edges: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    
    pub fn add(&mut self, node: N) {
        if self.find(&node).is_none() {
            self.push(node);
        }
    }

    pub fn link(&mut self, node: N, child: N) -> &mut E {
        let n = self.find_or_insert(node);
        let c = self.find_or_insert(child);
        if let Some(i) = self.find_edge(n, c) {
            &mut self.edges[n][i]
        } else {
            self.graph[n].push(c);
            self.edges[n].push(E::default());
            self.edges[n].last_mut().unwrap()
        }
    }

    pub fn contains<Q: ?Sized>(&self, k: &Q) -> bool
    where
        N: Borrow<Q>,
        Q: Eq,
    {
        let b = k.borrow();
        self.nodes.iter().any(|v| v.borrow().eq(b))
    }

    pub fn nodes(&self) -> impl Iterator<Item = &N> {
        self.nodes.iter()
    }

    pub fn edge(&self, from: &N, to: &N) -> Option<&E> {
        let n = self.find(from)?;
        let c = self.find(to)?;
        let e = self.find_edge(n, c)?;
        Some(&self.edges[n][e])
    }

    pub fn edges(&self, from: &N) -> impl Iterator<Item = (&N, &E)> {
        let f = self.find(from).expect("Node `from` not in graph");
        let e = &self.edges[f];
        self.graph[f].iter()
            .enumerate()
            .map(move |(i, n)| (&self.nodes[*n], &e[i]))
    }

    pub fn cycles(&self) -> GraphCycles {
        GraphCycles(detect_cycles(self))
    }

    pub fn parallel_stages(&self) -> GraphParallelIter<N, E> {
        GraphParallelIter::new(self)
    }

    fn find(&self, node: &N) -> Option<usize> {
        self.nodes.iter().position(|v| v == node)
    }

    fn find_edge(&self, n: usize, c: usize) -> Option<usize> {
        self.graph[n].iter().position(|v| *v == c)
    }

    fn find_or_insert(&mut self, node: N) -> usize {
        self.find(&node)
            .unwrap_or_else(|| self.push(node))
    }

    fn push(&mut self, node: N) -> usize {
        self.nodes.push(node);
        self.edges.push(Vec::new());
        self.graph.push(Vec::new());
        self.nodes.len() - 1
    }
}

impl<N: fmt::Display + Eq + Clone, E: Clone> fmt::Debug for Graph<N, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "Graph {{")?;
        for (n, e) in self.graph.iter().enumerate() {
            writeln!(fmt, "  - {}", self.nodes[n])?;
            for n in e.iter() {
                writeln!(fmt, "    - {}", self.nodes[*n])?;
            }
        }
        write!(fmt, "}}")?;
        Ok(())
    }
}


/// Helper struct for managing cycles in a graph
pub struct GraphCycles(Vec<Vec<usize>>);

impl GraphCycles {
    // Returns nested iterator X of iterators (Y, Y1, Y2, ...)
    //  - Item of Y has lifetime of graph borrow ('b)
    //  - Y has lifetime of self borrow ('a)
    //  - graph borrow ('b) must be >= self borrow ('a)
    //      so that each item of Y lives at least as long as Y
    pub fn iter<'a, 'b, N: Clone, E: Clone>(
        &'a self, 
        g: &'b Graph<N, E>
    ) -> impl Iterator<Item = impl Iterator<Item = &'b N> + 'a> where 'b: 'a,
    {
        self.0
            .iter()
            .map(|v| {
                v.iter()
                    .map(|i| &g.nodes[*i])
            })
    }

    /// Remove cycles from a graph based on a heuristic and return removed edges.
    /// The current heuristic used is to remove an edge from the node 
    /// that has the least number of non-cycle links per cycle
    pub fn remove_from_graph<N: Clone, E: Clone>(self, g: &mut Graph<N, E>) -> GraphRemoved<E> {
        if self.0.is_empty() {
            return GraphRemoved(Vec::new());
        }
    
        let mut removed = Vec::new();
        let cycles = self.0;
        
        // Create bitset for fast `is_cycle` checks
        let mut is_cycle = BitVec::new();
        is_cycle.resize(g.nodes.len());    
        for cycle in cycles.iter() {
            for c in cycle.iter() {
                is_cycle.set(*c);
            }
        }
        
        // Remove cycles from graph
        for cycle in cycles.into_iter() {
            // Find node with least number of non-cycle links
            let node = cycle
                .into_iter()
                .min_by_key(|n| {
                    g.graph[*n].iter()
                        .map(|i| is_cycle.get(*i))
                        .count()
                })
                .unwrap();
    
            // Remove link from that node within cycle
            let (c, e) = if g.graph[node].len() == 1 {
                // Only one link - just pop
                let c = g.graph[node].pop().unwrap();
                let e = g.edges[node].pop().unwrap();
                (c, e)
            } else {
                // Multiple links - find first link that is part of the cycle
                let index = g.graph[node]
                    .iter()
                    .position(|c| is_cycle.get(*c))
                    .unwrap();
                let c = g.graph[node].swap_remove(index);
                let e = g.edges[node].swap_remove(index);
                (c, e)
            };
    
            // Track which edge was removed
            removed.push((node, c, e));
        }
    
        GraphRemoved(removed)
    }

}


/// Helper struct for managing removed links in a graph
pub struct GraphRemoved<E: Clone> (Vec<(usize, usize, E)>);

impl<E: Clone> GraphRemoved<E> {
    pub fn iter<'a, N: Clone>(self, g: &'a Graph<N, E>) -> impl Iterator<Item = (&'a N, &'a N, E)> {
        self.0
            .into_iter()
            .map(|(l, r, e)| (&g.nodes[l], &g.nodes[r], e))
    }
}


/// Algorithm to decompose graph into lists of lists of nodes,
/// where all the nodes in each list can be processed in parallel
/// while maintaining the correct order of depdendent executions
/// NOTE: Only works for DAGs, use `remove_cycles` to convert to DAG.
pub struct GraphParallelIter<'a, N: Clone, E: Clone> {
    g: &'a Graph<N, E>,
    group: Vec<usize>,
    remaining: HashSet<usize>,
}

impl<'a, N: Clone, E: Clone> GraphParallelIter<'a, N, E> {
    pub fn new(g: &'a Graph<N, E>) -> Self {
        Self{
            g, 
            group: Vec::new(),
            remaining: HashSet::from_iter(0..g.nodes.len()),
        }
    }
}

impl<'a, N: Clone, E: Clone> Iterator for GraphParallelIter<'a, N, E> {
    type Item = Vec<&'a N>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        // Collect nodes that no longer have any pending dependencies
        self.group.clear();
        for n in self.remaining.iter() {
            if self.g.graph[*n].iter().all(|k| !self.remaining.contains(k)) {
                self.group.push(*n);
            }
        }
        // We remove from remaining after the loop to prevent adding 
        // nodes that depend on eachother to the same group        
        for n in self.group.iter() {
            self.remaining.remove(n);
        }
        Some(self.group.iter().map(|n| &self.g.nodes[*n]).collect())
    }
}


/// Tarjan's strongly connected components algorithm
/// Used to collect all cycles in a graph in linear time
///  --> https://en.wikipedia.org/wiki/Tarjan%27s_strongly_connected_components_algorithm
/// 
#[derive(Default)]
struct State {
    current: usize,
    stack: Vec<usize>,
    index: Vec<usize>,
    lowlink: Vec<usize>,
    on_stack: BitVec,
    cycles: Vec<Vec<usize>>,
}

fn detect_cycles<N: Clone, E: Clone>(g: &Graph<N, E>) -> Vec<Vec<usize>> {
    let mut s = State::default();    
    s.index.resize(g.nodes.len(), usize::MAX);
    s.lowlink.resize(g.nodes.len(), 0);
    s.on_stack.resize(g.nodes.len());
    for v in 0..g.nodes.len() {
        if s.index[v] == usize::MAX {
            strong_connect(g, &mut s, v);
        }
    }
    s.cycles
}

fn strong_connect<N: Clone, E: Clone>(g: &Graph<N, E>, s: &mut State, v: usize) {
    // Set the depth index for v to the smallest unused index
    s.index[v] = s.current;
    s.lowlink[v] = s.current;
    s.on_stack.set(v);
    s.current += 1;
    s.stack.push(v);

    for w in g.graph[v].iter() {
        let w = *w;
        if s.index[w] == usize::MAX {
            // Successor w has not yet been visited; recurse on it
            strong_connect(g, s, w);
            s.lowlink[v] = cmp::min(s.lowlink[v], s.lowlink[w]);
        } else if s.on_stack.get(w) {
            // Successor w is in stack S and hence in the current SCC
            // If w is not on stack, then (v, w) is an edge pointing to an SCC already found and must be ignored
            // Note: The next line may look odd - but is correct.
            // It says s.index[w] not s.lowlink[w]; that is deliberate and from the original paper
            s.lowlink[v] = cmp::min(s.lowlink[v], s.index[w])
        }
    }

    // If v is a root node, pop the stack and generate an SCC
    if s.lowlink[v] == s.index[v] {
        let mut cycle = Vec::new();
        while let Some(w) = s.stack.pop() {
            s.on_stack.clear(w);
            cycle.push(w);
            if w == v {
                break;
            }
        }
        // Cycles must have at least two nodes
        if cycle.len() > 1 {
            s.cycles.push(cycle);
        }
    }
}


/*
    let mut g: Graph<&'static str, usize> = Graph::new();

    g.add("a");
    g.add("b");
    g.add("c");
    g.add("d");

    *g.link("b", "a") = 1;
    *g.link("b", "c") = 2;
    *g.link("c", "b") = 3;
    *g.link("d", "b") = 4;

    let cycles = g.cycles();

    for c in cycles.iter(&g) {
        println!("Cycle {:?}", c.collect::<Vec<_>>());
    }

    let removed = cycles.remove_from_graph(&mut g);

    for stage in g.parallel_stages() {
        println!("{:?}", stage);
    }

    for (p, c, e) in removed.iter(&g) {
        println!("Removed {} -> {} = {}", p, c, e);
    }
*/



#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct BitVec {
    words: Vec<u64>,
}

impl BitVec {
    pub fn new() -> Self {
        Self{words: Vec::new()}
    }

    pub fn with_size(n_bits: usize) -> Self {
        let mut s = Self::new();
        s.resize(n_bits);
        s
    }

    pub fn resize(&mut self, n_bits: usize) {
        let n = (n_bits + N_BITS_PER_WORD - 1) / N_BITS_PER_WORD;
        self.words.resize(n, 0);
    }

    pub fn reset(&mut self) {
        self.words.fill(0);
    }

    #[inline]
    pub fn get(&self, i: usize) -> bool {
        (self.words[i / N_BITS_PER_WORD] & mask(i)) != 0
    }

    #[inline]
    pub fn set(&mut self, i: usize) {
        self.words[i / N_BITS_PER_WORD] |= mask(i);
    }

    #[inline]
    pub fn clear(&mut self, i: usize) {
        self.words[i / N_BITS_PER_WORD] &= !mask(i);
    }
}

impl std::fmt::Debug for BitVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for w in self.words.iter() {
            for i in 0..N_BITS_PER_WORD {
                if (w & (1 << i)) != 0 {
                    1
                } else {
                    0
                }.fmt(f)?
            }
        }
        Ok(())
    }
}

type Word = u64;
const N_BITS_PER_WORD: usize = std::mem::size_of::<Word>() * 8;

#[inline]
const fn mask(i: usize) -> u64 {
    1 << (i & (N_BITS_PER_WORD - 1))
}
