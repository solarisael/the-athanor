extends VBoxContainer
class_name AthanorHarnessesScreen

var _client: Node
var _selected_id := ""
var _harnesses: Array[Dictionary] = []

@onready var _state: Label = %State
@onready var _detail: Label = %Detail
@onready var _list: VBoxContainer = %HarnessList
@onready var _selected: Label = %Selected
@onready var _start: Button = %Start
@onready var _stop: Button = %Stop
@onready var _restart: Button = %Restart

func _ready() -> void:
	_client = get_node("HarnessControl")
	%Refresh.pressed.connect(_refresh)
	_start.pressed.connect(_act.bind("start"))
	_stop.pressed.connect(_act.bind("stop"))
	_restart.pressed.connect(_act.bind("restart"))
	_render_unavailable("waiting for the managed control owner")
	_refresh()

func _refresh() -> void:
	var raw: String = _client.list()
	if raw.is_empty():
		_render_client_error()
		return
	_apply_response(raw)

func _act(method: String) -> void:
	if _selected_id.is_empty():
		_state.text = "NO HARNESS SELECTED"
		_detail.text = "Select a managed harness before sending a request."
		return
	var raw: String = _client.call(method, _selected_id)
	if raw.is_empty():
		_render_client_error()
		return
	_apply_response(raw)

func _apply_response(raw: String) -> void:
	var decoded = JSON.parse_string(raw)
	if not decoded is Dictionary:
		_render_malformed("The managed control response is not a JSON object.")
		return
	var response: Dictionary = decoded
	if not response.has("ok") or not response.has("format"):
		_render_malformed("The managed control response has no outcome or format.")
		return
	if not bool(response.ok):
		_render_refused(str(response.get("error", "The managed control owner refused the request.")))
		return
	if not response.get("harnesses", []) is Array:
		_render_malformed("The managed control response has no harness list.")
		return
	_harnesses.clear()
	for item in response.harnesses:
		if not item is Dictionary or not item.has("harnessId") or not item.has("label") or not item.has("lifecycle"):
			_render_malformed("A harness entry is missing its identity or lifecycle.")
			return
		_harnesses.append(item)
	_render_harnesses()

func _render_harnesses() -> void:
	for child in _list.get_children():
		child.queue_free()
	if _harnesses.is_empty():
		_selected_id = ""
		_state.text = "EMPTY · NO MANAGED HARNESSES"
		_detail.text = "The parent owner returned an empty managed registry."
		_selected.text = "SELECTED HARNESS · NONE"
		_update_actions()
		return
	_state.text = "AVAILABLE · %d MANAGED HARNESSES" % _harnesses.size()
	_detail.text = "The parent owner controls every process. This screen sends requests only."
	var still_selected := false
	for harness in _harnesses:
		var id := str(harness.harnessId)
		still_selected = still_selected or id == _selected_id
		_list.add_child(_make_card(harness))
	if not still_selected:
		_selected_id = ""
	_selected.text = "SELECTED HARNESS · %s" % (_selected_id if not _selected_id.is_empty() else "NONE")
	_update_actions()

func _make_card(harness: Dictionary) -> PanelContainer:
	var card := PanelContainer.new()
	card.custom_minimum_size = Vector2(0, 112)
	card.add_theme_constant_override("separation", 8)
	card.theme_type_variation = &"AthanorVessel"
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	card.add_child(column)
	var title := Button.new()
	title.text = "%s  ·  %s" % [str(harness.label), str(harness.harnessId)]
	title.alignment = HORIZONTAL_ALIGNMENT_LEFT
	title.theme_type_variation = &"AthanorTabActive" if str(harness.harnessId) == _selected_id else &"AthanorTab"
	title.pressed.connect(_select.bind(str(harness.harnessId)))
	column.add_child(title)
	var facts := Label.new()
	facts.theme_type_variation = &"AthanorBody"
	facts.text = "LIFECYCLE  %s    PID  %s" % [str(harness.lifecycle).to_upper(), str(harness.get("pid", "—"))]
	column.add_child(facts)
	var detail := Label.new()
	detail.theme_type_variation = &"AthanorMeta"
	detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	detail.text = "DETAIL  %s" % str(harness.get("detail", "—"))
	column.add_child(detail)
	return card

func _select(harness_id: String) -> void:
	_selected_id = harness_id
	_selected.text = "SELECTED HARNESS · %s" % harness_id
	_update_actions()
	_render_harnesses()

func _update_actions() -> void:
	var selected_harness: Dictionary = {}
	for harness in _harnesses:
		if str(harness.get("harnessId", "")) == _selected_id:
			selected_harness = harness
	var lifecycle := str(selected_harness.get("lifecycle", ""))
	_start.disabled = selected_harness.is_empty() or lifecycle == "running"
	_stop.disabled = selected_harness.is_empty() or lifecycle != "running"
	_restart.disabled = selected_harness.is_empty()

func _render_client_error() -> void:
	var kind := str(_client.last_error_kind()).to_upper()
	var detail := str(_client.last_error_detail())
	if kind == "MALFORMED":
		_render_malformed(detail)
	else:
		_render_unavailable(detail)

func _render_unavailable(detail: String) -> void:
	_state.text = "UNAVAILABLE · MANAGED CONTROL OFFLINE"
	_detail.text = detail
	_update_actions()

func _render_malformed(detail: String) -> void:
	_state.text = "MALFORMED · CONTROL RESPONSE REJECTED"
	_detail.text = detail
	_update_actions()

func _render_refused(detail: String) -> void:
	_state.text = "REFUSED · OWNER DID NOT APPLY REQUEST"
	_detail.text = detail
	_update_actions()
