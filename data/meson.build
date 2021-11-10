datadir = get_option('prefix') / get_option('datadir')

app_id = 'com.github.HarmonyDevelopment.Loqui'

install_data(
	'@0@.desktop'.format(app_id),
	install_dir: datadir / 'applications'
)

install_data(
	'@0@.metainfo.xml'.format(app_id),
	install_dir: datadir / 'metainfo'
)

install_data(
	'@0@.svg'.format(app_id),
	install_dir: datadir / 'icons' / 'hicolor' / 'scalable' / 'apps'
)
