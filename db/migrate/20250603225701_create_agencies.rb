class CreateAgencies < ActiveRecord::Migration[8.0]
  def change
    create_table :agencies, id: false, force: true do |t|
      t.string :id, null: false, primary_key: true
      t.string :name, null: false
      t.string :url
      t.string :timezone, default: 'Europe/London'
      t.string :lang, default: 'EN'
      t.string :phone
      t.string :noc, null: false

      t.timestamps
    end
  end
end
