<script lang="ts">
	interface Props {
		width: number;
		height: number;
		faviconDataUrl: string;
	}

	let { width, height, faviconDataUrl }: Props = $props();
</script>

<div
	style="display:flex; position:relative; width:{width}px; height:{height}px; overflow:hidden; background:#0d1b2a; color:#e7f0f5; font-family:'IBM Plex Sans';">
	<div
		style="display:flex; position:absolute; inset:0; background:linear-gradient(125deg, #0d1b2a 0%, #132f49 58%, #153a66 100%);">
	</div>
	<div
		style="display:flex; position:absolute; top:-190px; right:-105px; width:550px; height:550px; border:1px solid #315673; border-radius:275px; opacity:0.72;">
	</div>
	<div
		style="display:flex; position:absolute; top:-120px; right:-35px; width:410px; height:410px; border:1px solid #315673; border-radius:205px; opacity:0.66;">
	</div>
	<div
		style="display:flex; position:absolute; right:106px; top:82px; width:12px; height:12px; border-radius:6px; background:#28b8aa;">
	</div>
	<div
		style="display:flex; position:absolute; right:159px; top:121px; width:7px; height:7px; border-radius:4px; background:#28b8aa; opacity:0.75;">
	</div>

	<div style="display:flex; position:relative; width:100%; flex-direction:column; padding:48px 58px 50px;">
		<div
			style="display:flex; align-items:center; gap:13px; color:#9ce3da; font-family:'Google Sans'; font-size:23px; font-weight:700; letter-spacing:-0.4px;">
			<img src={faviconDataUrl} alt="" style="display:flex; width:31px; height:31px;" />
			<span>Mire</span>
		</div>

		<div style="display:flex; flex: 1; margin-top:46px; flex-direction:column;">
			<div
				style="display:flex; color:#55d5c7; font-family:'Google Sans'; font-size:18px; font-weight:600; letter-spacing:2.2px;">
				LOCAL CODE REVIEW
			</div>
			<div
				style="display:flex; max-width:720px; margin-top:10px; color:#ffffff; font-family:'Google Sans'; font-size:72px; font-weight:700; letter-spacing:-2.8px; line-height:0.98;">
				Review code together.
			</div>
			<div style="display:flex; max-width:580px; margin-top:20px; color:#c6d7e2; font-size:25px; line-height:1.32;">
				A local review environment for humans and agents.
			</div>
		</div>

		<div
			style="display:flex;  justify-content: space-between; align-items:center; gap:17px; color:#c6d7e2; font-family:'IBM Plex Sans'; font-size:19px; line-height:1;">
			<div style="display:flex; align-items:center; gap:10px;">
				<div style="display:flex; width:8px; height:8px; border-radius:4px; background:#007f7c;"></div>
				<span>github.com/stormlightlabs/mire</span>
			</div>
			<div style="display:flex; align-items:center; gap:10px;">
				<div style="display:flex; width:8px; height:8px; border-radius:4px; background:#55d5c7;"></div>
				<span>mire.stormlightlabs.org</span>
			</div>
		</div>
	</div>
</div>
