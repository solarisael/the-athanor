@tool
class_name JuiceeGraphNodeData
extends Resource

@export var id: String = ""
@export var type: String = ""
@export var graph_position: Vector2 = Vector2.ZERO
@export var effect: JuiceeEffect = null
## Optional per-node properties (used by builtins like loop count, random weights).
@export var properties: Dictionary = {}

static func create_for_builtin(p_type: String, p_position: Vector2) -> JuiceeGraphNodeData:
	var data := JuiceeGraphNodeData.new()
	data.id = "%s_%d" % [p_type, Time.get_ticks_msec() + randi() % 1000]
	data.type = p_type
	data.graph_position = p_position
	data.properties = _builtin_defaults(p_type)
	return data

static func _builtin_defaults(p_type: String) -> Dictionary:
	match p_type:
		"loop":    return {"count": 3}
		"split":   return {"port_count": 3}
		"random":  return {"port_count": 3, "weights": [1.0, 1.0, 1.0]}
		"comment": return {"text": "Comment"}
		_:         return {}

static func create_for_effect(effect_script: Script, p_position: Vector2) -> JuiceeGraphNodeData:
	var data := JuiceeGraphNodeData.new()
	var script_name: String = effect_script.resource_path.get_file().get_basename()
	data.id = "%s_%d" % [script_name, Time.get_ticks_msec() + randi() % 1000]
	data.type = "effect"
	data.graph_position = p_position
	data.effect = effect_script.new()
	data.effect.graph_position = p_position
	return data
