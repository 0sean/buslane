class CreateCalendarDates < ActiveRecord::Migration[8.0]
  def change
    create_table :calendar_dates do |t|
      t.references :service, null: false
      t.date :date, null: false
      t.integer :exception_type, null: false

      t.timestamps
    end
  end
end
