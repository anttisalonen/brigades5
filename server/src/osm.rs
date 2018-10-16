extern crate osm_xml;
extern crate geo;

use std::fs::File;
use geo::*;

struct MapData {
	roads: Vec<LineString<f64>>,
	terrain: Vec<MultiPolygon<f64>>,
	bounds: Coordinate<f64>
}

pub fn run_osm() {
	let f = File::open("tmp/res.osm").unwrap();
	let doc = osm_xml::OSM::parse(f).unwrap();
	let rel_info = relation_reference_statistics(&doc);
	let way_info = way_reference_statistics(&doc);
	let poly_count = doc.ways.values().fold(0, |acc, way| {
		if way.is_polygon() {
			return acc + 1
		}

		acc
	});

	println!("Node count {}", doc.nodes.len());
	println!("Way count {}", doc.ways.len());
	println!("Polygon count {}", poly_count);
	println!("Relation count {}", doc.relations.len());
	println!("Tag count {}", tag_count(&doc));

	println!("Way reference count: {}, invalid references: {}",  way_info.0, way_info.1);
	println!("Relation reference count: {}, resolved: {}, unresolved: {}", rel_info.0, rel_info.1, rel_info.2);

	let map = gen_map_data(&doc);
	println!("have map: {}\n", map.roads.len());
	println!("have map: {:?}\n", map.roads);
	println!("bounds: {:?}\n", map.bounds);
}

fn way_is_highway(way: &osm_xml::Way) -> bool {
	return way.tags.to_owned().into_iter().any(move |t| t.key == "highway");
}

fn resolve_node(doc: &osm_xml::OSM, node: osm_xml::UnresolvedReference) -> Option<osm_xml::Node> {
	match doc.resolve_reference(&node) {
		osm_xml::Reference::Node(n) => Some(n.to_owned()),
		_                           => None
	}
}

fn node_to_coordinate(center: Coordinate<f64>, node: osm_xml::Node) -> Coordinate<f64> {
	let c = normalise_latlong(Coordinate { x: node.lat, y: node.lon});
	Coordinate {
		x: c.x - center.x,
		y: c.y - center.y
	}
}

fn resolve_references(doc: &osm_xml::OSM, center: Coordinate<f64>, way: osm_xml::Way) -> Option<LineString<f64>> {
	let items: Option<Vec<_>> = way.nodes.into_iter()
		.map(|n| resolve_node(doc, n))
		.collect();
	items.and_then(|i| Some(i.into_iter().map(|n| node_to_coordinate(center, n)).collect()))
}

fn normalise_latlong(c: Coordinate<f64>) -> Coordinate<f64> {
	Coordinate {
		x: c.x * 111320.,
		y: c.y * 111320. * c.x.to_radians().cos()
	}
}

fn find_bounds(doc: &osm_xml::OSM) -> osm_xml::Bounds {
	let val = doc.nodes.values()
		.fold((180.0_f64, 180.0_f64, -180.0_f64, -180.0_f64),
		|(minlat, minlon, maxlat, maxlon), n|
		(minlat.min(n.lat),
		minlon.min(n.lon),
		maxlat.max(n.lat),
		maxlon.max(n.lon)));
	let v1 = normalise_latlong(Coordinate { x: val.0, y: val.1 });
	let v2 = normalise_latlong(Coordinate { x: val.2, y: val.3 });
	osm_xml::Bounds {
		minlat: v1.x,
		minlon: v1.y,
		maxlat: v2.x,
		maxlon: v2.y,
	}
}

fn find_center(bounds: osm_xml::Bounds) -> Coordinate<f64> {
	Coordinate {
		x: (bounds.minlat + bounds.maxlat) / 2.,
		y: (bounds.minlon + bounds.maxlon) / 2.
	}
}

fn gen_map_data(doc: &osm_xml::OSM) -> MapData {
	let bounds = match doc.bounds {
		Some(b) => b,
		None    => find_bounds(doc)
	};

	let center = find_center(bounds);
	MapData {
		roads: doc.ways.values()
			.filter(|way| (way_is_highway(way)))
			.map(|way| resolve_references(doc, center, way.to_owned()))
			.flatten()
			.collect(),
		terrain: [].to_vec(),
		bounds: Coordinate {
			x: bounds.maxlat - bounds.minlat,
			y: bounds.maxlon - bounds.minlon
		}
	}
}

fn relation_reference_statistics(doc: &osm_xml::OSM) -> (usize, usize, usize) {
	doc.relations.values()
		.flat_map(|relation| relation.members.iter())
		.fold((0, 0, 0), |acc, member| {
			let el_ref = match *member {
				osm_xml::Member::Node(ref el_ref, _) => el_ref,
				osm_xml::Member::Way(ref el_ref, _) => el_ref,
				osm_xml::Member::Relation(ref el_ref, _) => el_ref,
			};

			match doc.resolve_reference(&el_ref) {
				osm_xml::Reference::Unresolved => (acc.0 + 1, acc.1, acc.2 + 1),
				osm_xml::Reference::Node(_)     |
					osm_xml::Reference::Way(_)      |
					osm_xml::Reference::Relation(_) => (acc.0 + 1, acc.1 + 1, acc.2)
			}
		})
}

fn way_reference_statistics(doc: &osm_xml::OSM) -> (usize, usize) {
	doc.ways.values()
		.flat_map(|way| way.nodes.iter())
		.fold((0, 0), |acc, node| {
			match doc.resolve_reference(&node) {
				osm_xml::Reference::Node(_) => (acc.0 + 1, acc.1),
				osm_xml::Reference::Unresolved  |
					osm_xml::Reference::Way(_)      |
					osm_xml::Reference::Relation(_) => (acc.0, acc.1 + 1)
			}
		})
}

fn tag_count(doc: &osm_xml::OSM) -> usize {
	let node_tag_count = doc.nodes.values()
		.map(|node| node.tags.len())
		.fold(0, |acc, c| acc + c);
	let way_tag_count = doc.ways.values()
		.map(|way| way.tags.len())
		.fold(0, |acc, c| acc + c);
	let relation_tag_count = doc.relations.values()
		.map(|relation| relation.tags.len())
		.fold(0, |acc, c| acc + c);

	node_tag_count + way_tag_count + relation_tag_count
} 
