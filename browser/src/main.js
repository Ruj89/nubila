let db, editor, documents;
let currentObject = window.crypto.randomUUID();
let currentName = undefined;

const $ = window.$;
import * as monaco from 'https://esm.sh/monaco-editor';

function displayDocuments() {
	$("ul#file_list").remove();
	const objectStore = db.transaction(["documents"]).objectStore("documents");
	objectStore.getAllKeys().onsuccess = (event) => {
		documents = event?.target?.result;
		if (documents?.length ?? 0 > 0) {
			var documentListDom = document.createElement("ul");
			documentListDom.id = "file_list";
			for (const documentId of documents) {
				const documentRequest = objectStore.get(documentId);
				documentRequest.onsuccess = () => {
					var documentListElementDom = document.createElement("li");
					documentListElementDom.innerHTML = documentRequest?.result?.name ?? "Unsupported";
					documentListElementDom.onclick = () => openDocument(documentId)
					$(documentListDom).append(documentListElementDom);
				};
			}
			$("article").prepend(documentListDom);
		}
	};
}

function openDocument(documentId) {
	const objectStore = db.transaction(["documents"]).objectStore("documents");
	const documentRequest = objectStore.get(documentId);
	documentRequest.onsuccess = () => {
		$("#document-name").val(documentRequest?.result?.name);
		editor?.setValue(documentRequest?.result?.content);
		currentObject = documentId;
		currentName = documentRequest?.result?.name;
	}
}

function saveDocument() {
	try {
		const name = $("#document-name").val();
		if ((name ?? "") == "") {
			console.error("No name inserted");
			return;
		}
		if (currentName != name) {
			currentObject = window.crypto.randomUUID();
		}
		let content = editor?.getValue() ?? "";
		const objectStore = db.transaction(["documents"], "readwrite").objectStore("documents");
		objectStore.put({ uuid: currentObject, content, name });
		displayDocuments();
	} catch (ex) {
		console.error(ex);
	}
}

function deleteDocument() {
	if (currentName == undefined) {
		console.error("No file opened");
		return;
	}	
	const objectStore = db.transaction(["documents"], "readwrite").objectStore("documents");
	objectStore.delete(currentObject);
	currentObject = window.crypto.randomUUID();
	currentName = undefined;
	$("#document-name").val("");
	$("textarea[name='document-content']").val("");
	editor?.setValue("");
	displayDocuments();
}

function configureMonaco() {
	let theme = "vs";
	if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
		theme = "vs-dark";
	}
	editor = monaco.editor.create(document.getElementById("editor"), {
		value: '',
		language: 'markdown',
		lineNumbers: "off",
		wordWrap: "bounded",
		wordWrapColumn: 100,
		wrappingIndent: "same",
		fontSize: 14,
		roundedSelection: false,
		scrollBeyondLastLine: false,
		quickSuggestions: false,
		minimap: { enabled: false },
		theme: theme,
	});
	$("textarea[name='document-content']").css("display", "none");

	window.addEventListener("keydown", function(e) {
		if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
			e.preventDefault();
		}
	});
	editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, function () {
		$("textarea[name='document-content']").val(editor.getValue());
		$("form#save-form").submit();
	});

	window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", e => {
		if (e.matches) {
			monaco.editor.setTheme("vs-dark");
		} else {
			monaco.editor.setTheme("vs");
		}
	})

	const divElem = document.getElementById("editor");
	const resizeObserver = new ResizeObserver(() => {
		editor.layout();
	});
	resizeObserver.observe(divElem);
}


function openDatabase() {
	const request = indexedDB.open("MyTestDatabase");
	request.onerror = (event) => {
		console.error(`Database error: ${event.target.error?.message}`);
	};
	request.onsuccess = (event) => {
		try {
			$("form#save-form").submit((e) => {
				switch(e?.originalEvent?.submitter?.name) {
					case "delete":
						deleteDocument();
						break;
					case "save":
					default:
						saveDocument();
						break;

				}
				return false;
			});
			db = event.target.result;
			displayDocuments();
		} catch (ex) {
			console.error(ex);
		}
	};
	request.onupgradeneeded = (event) => {
		try {
			db = event.target.result;
			db.createObjectStore("documents", { keyPath: "uuid" });
			db.createIndex("name", "name", { unique: true });
		} catch (ex) {
			console.error(ex);
		}
	};
}

configureMonaco();
openDatabase();