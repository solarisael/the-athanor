extends SceneTree

## Smoke: the live shell boots with the real S01 center scene mounted, the shell
## feeds it the Host receipt it actually has, and Send refuses out loud.

const MAIN_SCENE: PackedScene = preload("res://main.tscn")

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	root.size = Vector2i(1440, 900)
	var shell := MAIN_SCENE.instantiate()
	root.add_child(shell)
	await process_frame
	await process_frame

	# `ready` docks the content viewport inside the shared center scroll.
	var page := shell.get_node_or_null("ApplicationShell/Workspace/CenterFrame/CenterScroll/ContentViewport/Page")
	if page == null:
		push_error("SHELL BOOT SMOKE: S01 center page not found")
		quit(1)
		return
	print("S01 CENTER CLASS: ", page.get_script().resource_path)
	print("S01 IS CHAT CENTER: ", page is S01ChatCenter)
	print("S01 VISIBLE: ", page.visible, " · MESSAGES: ", page.message_count())

	var receipt := page.get_node("%Receipt") as ReceiptCard
	print("RECEIPT COLLAPSED: ", receipt.collapsed)
	print("RECEIPT SUMMARY: ", receipt.get_node("Column/Summary/Value").text)
	print("RECEIPT TITLE: ", receipt.title_text)
	print("RECEIPT DELIVERED: ", receipt.delivered_text)

	var composer := page.get_node("%Composer") as Composer
	print("COMPOSER REFUSAL: ", composer.submit_refusal())

	var center_scroll := shell.get_node("ApplicationShell/Workspace/CenterFrame/CenterScroll") as ScrollContainer
	print("CENTER SCROLL MODE ON S01: ", center_scroll.vertical_scroll_mode)

	shell.queue_free()
	await process_frame
	print("SHELL BOOT SMOKE: DONE")
	quit(0)
